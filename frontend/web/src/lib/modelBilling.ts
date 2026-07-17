/** Keep LiteRouter model suffix consistent with billing mode. */
export function reconcileModelForBilling(
  model: string,
  billingMode: "free" | "paid",
  candidates: string[] = [],
): string {
  const current = model.trim();
  const isFree = current.endsWith(":free");
  const unique = [...new Set(candidates.map((item) => item.trim()).filter(Boolean))];

  if (billingMode === "free") {
    if (isFree) {
      return current || "deepseek:free";
    }
    return unique.find((item) => item.endsWith(":free")) ?? "deepseek:free";
  }

  if (!isFree) {
    return current;
  }
  const stripped = current.replace(/:free$/u, "");
  if (unique.includes(stripped)) {
    return stripped;
  }
  return unique.find((item) => !item.endsWith(":free")) ?? stripped;
}
