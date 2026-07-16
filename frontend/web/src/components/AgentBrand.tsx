import { AgentMark } from "./AgentMark";

type AgentBrandProps = {
  title?: string;
  markSize?: "sm" | "md";
  as?: "h1" | "h2" | "div";
};

export function AgentBrand({ title = "EvoHime", markSize = "md", as = "h1" }: AgentBrandProps) {
  const TitleTag = as;
  return (
    <div className="agentBrand">
      <AgentMark size={markSize} />
      <TitleTag>{title}</TitleTag>
    </div>
  );
}
