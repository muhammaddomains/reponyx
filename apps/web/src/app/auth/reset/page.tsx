import { redirectWithQuery } from "../redirect";

export default async function AuthResetRedirectPage(props: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  await redirectWithQuery("/reset", props.searchParams);
}
