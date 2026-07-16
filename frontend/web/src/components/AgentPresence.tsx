type AgentPresenceProps = {
  busy?: boolean;
};

export function AgentPresence({ busy = false }: AgentPresenceProps) {
  return (
    <aside
      className={busy ? "agentPresence busy" : "agentPresence"}
      aria-label="EvoHime"
    >
      <div className="agentPresenceGlow" aria-hidden="true" />
      <div className="agentPresenceFigure">
        <img
          className="agentPresencePortrait"
          src="/brand/agent-avatar-256.webp"
          width={220}
          height={220}
          alt="EvoHime"
          draggable={false}
        />
        <span className={busy ? "agentPresenceAura active" : "agentPresenceAura"} aria-hidden="true" />
      </div>
      <p className="agentPresenceName">EvoHime</p>
      <p className="agentPresenceMood">{busy ? "работаю… хмф" : "слушаю"}</p>
    </aside>
  );
}
