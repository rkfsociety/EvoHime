type AgentPresenceProps = {
  busy?: boolean;
};

export function AgentPresence({ busy = false }: AgentPresenceProps) {
  return (
    <aside
      className={busy ? "agentPresence busy" : "agentPresence"}
      aria-label="EvoHime"
      aria-hidden="true"
    >
      <img
        className="agentPresenceBody"
        src="/brand/agent-body-720.webp"
        alt=""
        draggable={false}
      />
    </aside>
  );
}
