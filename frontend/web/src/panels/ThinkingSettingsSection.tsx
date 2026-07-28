import { useCallback, useEffect, useState } from "react";
import { getThinkingSettings, putThinkingSettings } from "../api/thinking";

export interface ThinkingSettings {
  enabled: boolean;
  budget_tokens: number | null;
  max_budget_tokens: number | null;
  show_thinking: boolean;
  thinking_verbosity: string | null;
  monthly_cost_limit_usd: number | null;
  warning_threshold_percent: number | null;
  monthly_spending_usd: number | null;
}

export function ThinkingSettingsSection() {
  const [settings, setSettings] = useState<ThinkingSettings | null>(null);
  const [draft, setDraft] = useState<Partial<ThinkingSettings>>({});
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [isDirty, setIsDirty] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getThinkingSettings();
      setSettings(data);
      setDraft({});
      setIsDirty(false);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleChange = (field: keyof ThinkingSettings, value: unknown) => {
    setDraft((prev) => ({ ...prev, [field]: value }));
    setIsDirty(true);
    setError(null);
    setNotice(null);
  };

  const handleSave = async () => {
    if (!settings) return;

    const merged = { ...settings, ...draft };

    // Validate budget tokens
    if (merged.budget_tokens !== null && merged.budget_tokens !== undefined) {
      if (merged.budget_tokens < 1000 || merged.budget_tokens > 32000) {
        setError("Бюджет должен быть между 1000 и 32000 токенов");
        return;
      }
    }

    setSaving(true);
    setError(null);
    setNotice(null);

    try {
      const updated = await putThinkingSettings({
        enabled: merged.enabled,
        budget_tokens: merged.budget_tokens || undefined,
        show_thinking: merged.show_thinking,
        thinking_verbosity: merged.thinking_verbosity || undefined,
      });
      setSettings(updated);
      setDraft({});
      setIsDirty(false);
      setNotice("Настройки сохранены");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setDraft({});
    setIsDirty(false);
    setError(null);
    setNotice(null);
  };

  const current = settings ? { ...settings, ...draft } : null;
  const spendingPercent = current && current.monthly_cost_limit_usd && current.monthly_spending_usd
    ? Math.min(100, (current.monthly_spending_usd / current.monthly_cost_limit_usd) * 100)
    : 0;
  const warningLevel = current?.warning_threshold_percent || 80;

  return (
    <section className="settingsSection thinkingSettings">
      <div className="settingsInlineBar">
        <div>
          <h3>Расширенное мышление (Wave 3B)</h3>
          <p className="settingsHint">
            Настройки бюджета токенов для расширенного мышления. Улучшает качество рассуждений.
          </p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Обновление…" : "Обновить"}
        </button>
      </div>

      {error ? <p className="settingsError">{error}</p> : null}
      {notice ? <p className="settingsSuccess">{notice}</p> : null}

      {!current ? (
        <p className="settingsHint">Загрузка…</p>
      ) : (
        <>
          {/* Enable/Disable toggle */}
          <div className="settingsGroup">
            <label>
              <input
                type="checkbox"
                checked={current.enabled}
                onChange={(e) => handleChange("enabled", e.target.checked)}
              />
              <span>Включить расширенное мышление</span>
            </label>
            <p className="settingsHint">
              Позволяет Claude более глубоко размышлять о сложных задачах перед ответом.
            </p>
          </div>

          {current.enabled && (
            <>
              {/* Budget tokens slider */}
              <div className="settingsGroup">
                <label htmlFor="budgetTokens">
                  Бюджет токенов мышления: <strong>{current.budget_tokens || 5000}</strong>
                </label>
                <input
                  id="budgetTokens"
                  type="range"
                  min="1000"
                  max="32000"
                  step="1000"
                  value={current.budget_tokens || 5000}
                  onChange={(e) => handleChange("budget_tokens", parseInt(e.target.value))}
                  className="settingsSlider"
                />
                <p className="settingsHint">
                  Больше токенов = более глубокие рассуждения, но медленнее и дороже.
                  Диапазон: 1000–32000 токенов.
                </p>
              </div>

              {/* Show thinking toggle */}
              <div className="settingsGroup">
                <label>
                  <input
                    type="checkbox"
                    checked={current.show_thinking}
                    onChange={(e) => handleChange("show_thinking", e.target.checked)}
                  />
                  <span>Показывать процесс мышления</span>
                </label>
                <p className="settingsHint">
                  Если включено, промежуточные шаги мышления будут видны в интерфейсе.
                </p>
              </div>

              {/* Thinking verbosity */}
              <div className="settingsGroup">
                <label htmlFor="verbosity">Уровень детализации мышления:</label>
                <select
                  id="verbosity"
                  value={current.thinking_verbosity || "compact"}
                  onChange={(e) => handleChange("thinking_verbosity", e.target.value)}
                  className="settingsSelect"
                >
                  <option value="compact">Компактно (основные шаги)</option>
                  <option value="full">Полно (все шаги и рассуждения)</option>
                  <option value="summary">Краткое резюме (только выводы)</option>
                </select>
              </div>

              {/* Cost tracking */}
              {current.monthly_spending_usd !== null && current.monthly_cost_limit_usd !== null && (
                <div className="settingsGroup">
                  <p>
                    <strong>Месячные расходы:</strong> ${current.monthly_spending_usd.toFixed(2)} из ${current.monthly_cost_limit_usd.toFixed(2)}
                  </p>
                  <div className="spendingGauge">
                    <div
                      className={`spendingBar ${spendingPercent >= warningLevel ? "warning" : ""}`}
                      style={{ width: `${spendingPercent}%` }}
                    />
                  </div>
                  <p className="settingsHint">
                    Предупреждение при {warningLevel}% от лимита. Расходы обновляются в реальном времени.
                  </p>
                </div>
              )}
            </>
          )}

          {/* Save/Cancel buttons */}
          {isDirty && (
            <div className="settingsActions">
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={saving}
                className="primaryButton"
              >
                {saving ? "Сохранение…" : "Сохранить"}
              </button>
              <button
                type="button"
                onClick={handleCancel}
                disabled={saving}
                className="secondaryButton"
              >
                Отменить
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}
