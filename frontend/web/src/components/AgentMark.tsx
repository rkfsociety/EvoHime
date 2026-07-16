type AgentMarkProps = {
  size?: "sm" | "md";
  className?: string;
};

const sizePx: Record<NonNullable<AgentMarkProps["size"]>, number> = {
  sm: 24,
  md: 32,
};

export function AgentMark({ size = "md", className }: AgentMarkProps) {
  const px = sizePx[size];
  return (
    <img
      className={["agentMark", className].filter(Boolean).join(" ")}
      src="/brand/agent-mark.svg"
      width={px}
      height={px}
      alt=""
      aria-hidden="true"
      draggable={false}
    />
  );
}
