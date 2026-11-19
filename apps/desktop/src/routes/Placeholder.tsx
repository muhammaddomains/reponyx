import { Card } from "@reponyx/ui";

/** Honest Phase-1 placeholder: states what is missing and in which phase
 *  it arrives. No fake buttons, no mock data, no pretend functionality. */
export default function Placeholder({
  title,
  phase,
}: {
  title: string;
  phase: number;
}) {
  return (
    <div className="space-y-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">{title}</h1>
        <p className="text-sm text-zinc-500">
          Not yet implemented — Phase {phase}.
        </p>
      </header>
      <Card className="p-4">
        <p className="text-sm text-zinc-600 dark:text-zinc-400">
          This section is planned for Phase {phase} (see{" "}
          <code>ai/phase{phase}/PLAN.md</code>). Nothing here is functional
          yet, and no data shown on this page would be real.
        </p>
      </Card>
    </div>
  );
}
