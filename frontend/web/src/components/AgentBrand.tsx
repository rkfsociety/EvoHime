import { AgentMark } from "./AgentMark";

type AgentBrandProps = {
  title?: string;
  markSize?: "sm" | "md";
  as?: "h1" | "h2" | "div";
  titleId?: string;
};

export function AgentBrand({ title = "EvoHime", markSize = "md", as = "h1", titleId }: AgentBrandProps) {
  const TitleTag = as;
  return (
    <div className="agentBrand">
      <AgentMark size={markSize} />
      <TitleTag id={titleId}>{title}</TitleTag>
    </div>
  );
}
