// Currency formatting lives only in the GUI — money_core never formats for
// display, only the CLI (tabled) and this (Intl.NumberFormat) do.
const formatter = new Intl.NumberFormat("es-MX", {
  style: "currency",
  currency: "MXN",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

export function formatMoney(amount: number): string {
  return formatter.format(amount);
}

export function Money({ amount, className }: { amount: number; className?: string }) {
  return <span className={className}>{formatMoney(amount)}</span>;
}
