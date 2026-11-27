import { redirectWithQuery } from "../redirect";

export default async function AuthVerifyRedirectPage(props: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  await redirectWithQuery("/verify", props.searchParams);
}
