import ui from "./ui.module.css";

export function ErrorBanner({ message }: { message: string | null | undefined }) {
  if (!message) return null;
  return <div className={ui.error}>{message}</div>;
}
