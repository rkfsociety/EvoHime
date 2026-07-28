import { useCallback, useEffect, useState } from "react";
import { getCostLimits, putCostLimits, type CostLimit, type CostTracking } from "../api/models";

export function SpendSettingsSection() {
  const [costLimits, setCostLimits] = useState<CostLimit[]>([]);
  const [costTracking, setCostTracking] = useState<CostTracking[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [draftLimits, setDraftLimits] = useState<Map<number, CostLimit>>(new Map());

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await getCostLimits();
      setCostLimits(response.limits);
      setCostTracking(response.tracking);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleLimitChange = (limit: CostLimit, field: keyof CostLimit, value: unknown) => {
    const draft = draftLimits.get(limit.id) || { ...limit };
    (draft[field] as unknown) = value;
    setDraftLimits(new Map(draftLimits.set(limit.id, draft)));
  };

  const handleSaveLimit = async (limit: CostLimit) => {
    const draft = draftLimits.get(limit.id) || limit;
    setSaving(true);
    setError(null);
    setNotice(null);

    try {
      const response = await putCostLimits({
        limits: costLimits.map((l) => (l.id === limit.id ? draft : l)),
      });
      setCostLimits(response.limits);
      draftLimits.delete(limit.id);
      setDraftLimits(new Map(draftLimits));
      setNotice("Лимит сохранён");
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const getSpendForModel = (model: string): number => {
    return costTracking
      .filter((t) => t.model === model)
      .reduce((sum, t) => sum + t.tokens_consumed, 0);
  };

  const getTodaySpend = (model: string): number => {
    const today = new Date().toISOString().split("T")[0];
    return (
      costTracking
        .filter((t) => t.model === model && t.date === today)
        .reduce((sum, t) => sum + t.tokens_consumed, 0) || 0
    );
  };

  return (
    <section className="settingsSection spendSettings">
      <div className="settingsInlineBar">
        <div>
          <h3>Лимиты стоимости</h3>
          <p className="settingsHint">
            Установи дневные лимиты токенов на модели. Отслеживание в режиме реального времени.
          </p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Обновление…" : "Обновить"}
        </button>
      </div>

      {error ? <p className="settingsError">{error}</p> : null}
      {notice ? <p className="settingsHint">{notice}</p> : null}

      <div className="spendLimitsTable">
        {costLimits.length === 0 ? (
          <p className="settingsHint">Нет настроенных моделей.</p>
        ) : (
          costLimits.map((limit) => {
            const draft = draftLimits.get(limit.id) || limit;
            const todaySpend = getTodaySpend(limit.model);
            const cappedPercentage = Math.min(
              100,
              (todaySpend / limit.daily_cap_tokens) * 100,
            );

            return (
              <div key={limit.id} className="spendLimitRow">
                <div className="spendModelInfo">
                  <strong>{limit.model}</strong>
                  <div className="spendGauge">
                    <div
                      className="spendGaugeFill"
                      style={{ width: `${cappedPercentage}%` }}
                    />
                  </div>
                  <span className="spendStatus">
                    {todaySpend.toLocaleString()} / {limit.daily_cap_tokens.toLocaleString()} токенов
                    {todaySpend > limit.daily_cap_tokens ? " (⚠ превышен)" : ""}
                  </span>
                </div>

                <div className="spendLimitEditor">
                  <label>
                    <span>Дневной лимит (токены)</span>
                    <input
                      type="number"
                      value={draft.daily_cap_tokens}
                      onChange={(e) =>
                        handleLimitChange(limit, "daily_cap_tokens", parseInt(e.target.value, 10))
                      }
                      min="1000"
                      step="1000"
                      disabled={saving}
                    />
                  </label>

                  <label>
                    <span>Час сброса UTC</span>
                    <input
                      type="number"
                      value={draft.reset_hour}
                      onChange={(e) =>
                        handleLimitChange(limit, "reset_hour", parseInt(e.target.value, 10))
                      }
                      min="0"
                      max="23"
                      disabled={saving}
                    />
                  </label>

                  <label className="checkboxLabel">
                    <input
                      type="checkbox"
                      checked={draft.enabled}
                      onChange={(e) => handleLimitChange(limit, "enabled", e.target.checked)}
                      disabled={saving}
                    />
                    <span>Лимит включён</span>
                  </label>

                  <button
                    type="button"
                    onClick={() => void handleSaveLimit(limit)}
                    disabled={saving || JSON.stringify(draft) === JSON.stringify(limit)}
                  >
                    {saving ? "Сохранение…" : "Сохранить"}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>

      <h4 className="spendSubheading">Использование (последние 30 дней)</h4>
      <div className="spendChartPlaceholder">
        <p className="settingsHint">
          График будет добавлен в волне 3. Пока показываем только текущие данные выше.
        </p>
      </div>
    </section>
  );
}
