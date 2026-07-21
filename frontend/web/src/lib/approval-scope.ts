export function isRememberableApprovalScope(scope: string) {
  const trimmed = scope.trim();
  return (
    trimmed.length > 0
    && trimmed !== "workspace"
    && !trimmed.startsWith("http://")
    && !trimmed.startsWith("https://")
  );
}
