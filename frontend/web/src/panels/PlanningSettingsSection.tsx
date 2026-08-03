import type { ModelRouteDraft } from "../types";

type ProviderModelPickerProps = {
  route: ModelRouteDraft;
  models: string[];
  onUpdate: (patch: Partial<ModelRouteDraft>) => void;
};

function ProviderModelPicker({ route, models, onUpdate }: ProviderModelPickerProps) {
  return (
    <div className="modelProviderForm">
      <label>
        <span>Провайдер</span>
        <select
          value={route.provider}
          onChange={(event) => {
            const provider = event.target.value;
            onUpdate({
              provider,
              base_url: provider === "literouter" ? "https://api.literouter.com/v1" : "https://api.openai.com/v1",
              // No hardcoded model on provider switch — pick from the live list
              // fetched for the newly selected provider once it loads.
              model: "",
              billing_mode: provider === "literouter" ? "free" : "paid",
            });
          }}
        >
          <option value="literouter">LiteRouter</option>
          <option value="openai-compatible">OpenAI-compatible</option>
        </select>
      </label>
      <label>
        <span>Модель</span>
        <select value={route.model} onChange={(event) => onUpdate({ model: event.target.value })}>
          {[route.model, ...models]
            .filter((model, index, list) => model && list.indexOf(model) === index)
            .map((model) => (
              <option key={model} value={model}>
                {model}
              </option>
            ))}
        </select>
      </label>
    </div>
  );
}

type PlanningSettingsSectionProps = {
  reviewerRoutes: Array<{ route: ModelRouteDraft; index: number }>;
  reviewerModels: string[];
  synthesizerRoute: ModelRouteDraft | null;
  synthesizerRouteIndex: number;
  synthesizerModels: string[];
  onSetReviewerCount: (count: number) => void;
  onUpdateModelDraft: (index: number, patch: Partial<ModelRouteDraft>) => void;
  onSave: () => void;
};

export function PlanningSettingsSection({
  reviewerRoutes,
  reviewerModels,
  synthesizerRoute,
  synthesizerRouteIndex,
  synthesizerModels,
  onSetReviewerCount,
  onUpdateModelDraft,
  onSave,
}: PlanningSettingsSectionProps) {
  return (
    <section className="settingsSection">
      <h3>Планирование</h3>
      <p className="settingsHint">
        Пул моделей, которые ревьюят спецификацию и план перед реализацией, и модель-синтезатор,
        сводящая их замечания в один отчёт.
      </p>

      <label>
        <span>Количество ревьюверов</span>
        <select
          value={reviewerRoutes.length || 1}
          onChange={(event) => onSetReviewerCount(Number(event.target.value))}
        >
          {[1, 2, 3, 4, 5].map((count) => (
            <option key={count} value={count}>
              {count}
            </option>
          ))}
        </select>
      </label>

      {reviewerRoutes.map(({ route, index }, position) => (
        <div key={route.name} className="orchestratorSettings">
          <h4>{`Ревьювер ${position + 1}`}</h4>
          <ProviderModelPicker
            route={route}
            models={reviewerModels}
            onUpdate={(patch) => onUpdateModelDraft(index, patch)}
          />
        </div>
      ))}

      {synthesizerRoute ? (
        <div className="orchestratorSettings">
          <h4>Синтезатор</h4>
          <ProviderModelPicker
            route={synthesizerRoute}
            models={synthesizerModels}
            onUpdate={(patch) => onUpdateModelDraft(synthesizerRouteIndex, patch)}
          />
        </div>
      ) : null}

      <button type="button" className="settingsSaveButton" onClick={onSave}>
        Сохранить
      </button>
    </section>
  );
}
