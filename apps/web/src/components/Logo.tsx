import Image from "next/image";

/** RepoNyx logo (from /public/logo.png) with an optional wordmark. */
export function Logo({
  size = 40,
  showWordmark = false,
  className = "",
}: {
  size?: number;
  showWordmark?: boolean;
  className?: string;
}) {
  return (
    <span className={`inline-flex items-center gap-2.5 ${className}`}>
      <Image
        src="/logo.png"
        alt="RepoNyx"
        width={size}
        height={size}
        className="shrink-0"
        priority
      />
      {showWordmark ? (
        <span className="text-lg font-semibold tracking-tight">RepoNyx</span>
      ) : null}
    </span>
  );
}
