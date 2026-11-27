import { redirectWithQuery } from "../redirect";

export default async function AuthLoginRedirectPage(props: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  await redirectWithQuery("/login", props.searchParams);
}
