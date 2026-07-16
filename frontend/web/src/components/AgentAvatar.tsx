type AgentAvatarProps = {
  size?: "sm" | "md" | "lg";
  className?: string;
};

const sizeSrc: Record<NonNullable<AgentAvatarProps["size"]>, { src: string; px: number }> = {
  sm: { src: "/brand/agent-avatar-64.webp", px: 28 },
  md: { src: "/brand/agent-avatar-128.webp", px: 40 },
  lg: { src: "/brand/agent-avatar-256.webp", px: 96 },
};

export function AgentAvatar({ size = "md", className }: AgentAvatarProps) {
  const { src, px } = sizeSrc[size];
  return (
    <img
      className={["agentAvatar", className].filter(Boolean).join(" ")}
      src={src}
      width={px}
      height={px}
      alt="EvoHime"
      draggable={false}
    />
  );
}
