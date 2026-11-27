export type Messages = Record<string, string>;
export type Translator = (
  key: string,
  vars?: Record<string, string | number>
) => string;

function interpolate(
  template: string,
  vars?: Record<string, string | number>
): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match
  );
}

/** Framework-free translator: `{name}` interpolation, `Intl.PluralRules`. */
export function createTranslator(messages: Messages): Translator {
  return (key, vars) => {
    const template = messages[key];
    if (template === undefined) return `[missing: ${key}]`;
    return interpolate(template, vars);
  };
}

export function formatPlural(
  locale: string,
  count: number,
  forms: Record<string, string>
): string {
  const rule = new Intl.PluralRules(locale).select(count);
  const template = forms[rule] ?? forms.other;
  if (template === undefined) return "[missing: plural]";
  return interpolate(template, { count });
}
