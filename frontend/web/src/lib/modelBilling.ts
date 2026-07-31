/**
 * Keep a LiteRouter model consistent with billing mode, using only models the
 * provider actually returned (`candidates`) — never a guessed/hardcoded model
 * name. If the current model already matches the requested mode, it's left
 * alone. If it doesn't and the real provider list has no matching model
 * (e.g. the catalog hasn't loaded yet), the current value is returned
 * unchanged rather than replaced with an invented one — an existing-but-
 * mismatched model is safer than a name that may not exist at all.
 */
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
      return current;
    }
    return unique.find((item) => item.endsWith(":free")) ?? current;
  }

  if (!isFree) {
    return current;
  }
  const stripped = current.replace(/:free$/u, "");
  if (unique.includes(stripped)) {
    return stripped;
  }
  return unique.find((item) => !item.endsWith(":free")) ?? current;
}
