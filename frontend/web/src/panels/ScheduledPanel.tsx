import { AgentMark } from "../components/AgentMark";

const scheduledRecommendations = [
  {
    icon: "♧",
    title: "Ежедневная сводка",
    schedule: "В будние дни в 8:00",
    description: "Начинайте каждый рабочий день со сводки календаря, непрочитанных писем и приоритетов",
    prompt: "Каждый будний день в 8:00 присылай мне сводку календаря, непрочитанных писем и приоритетов.",
  },
  {
    icon: "▤",
    title: "Еженедельный обзор",
    schedule: "По пятницам в 16:00",
    description: "Каждую пятницу создавайте краткий отчёт о проделанной работе",
    prompt: "Каждую пятницу в 16:00 создавай краткий отчёт о проделанной за неделю работе.",
  },
  {
    icon: "⌕",
    title: "Мониторинг дальнейших действий",
    schedule: "В будние дни в 9:00",
    description: "Проверяйте недавнюю активность в электронной почте и календаре и отмечайте всё, что требует вашего внимания",
    prompt: "Каждый будний день в 9:00 проверяй почту и календарь и отмечай действия, требующие моего внимания.",
  },
];

type ScheduledPanelProps = {
  onPickPrompt: (prompt: string) => void;
};

export function ScheduledPanel({ onPickPrompt }: ScheduledPanelProps) {
  return (
    <div className="scheduledPage">
      <section className="scheduledHero">
        <h2>Запланированные задачи</h2>
        <p className="scheduledHeroBrand">
          <AgentMark size="sm" />
          <span>Попросите EvoHime планировать задачи, ставить напоминания или отслеживать обновления</span>
        </p>
      </section>
      <section className="scheduledRecommendations">
        <h3>Рекомендации</h3>
        <div className="scheduledRecommendationList">
          {scheduledRecommendations.map((recommendation) => (
            <button
              type="button"
              className="scheduledRecommendation"
              key={recommendation.title}
              onClick={() => onPickPrompt(recommendation.prompt)}
            >
              <span className="scheduledRecommendationIcon" aria-hidden="true">{recommendation.icon}</span>
              <span className="scheduledRecommendationBody">
                <span className="scheduledRecommendationTitle">
                  <strong>{recommendation.title}</strong>
                  <em>{recommendation.schedule}</em>
                </span>
                <span>{recommendation.description}</span>
              </span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
