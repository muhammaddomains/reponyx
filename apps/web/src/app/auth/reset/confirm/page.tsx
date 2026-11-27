import { redirectWithQuery } from "../../redirect";

export default async function AuthResetConfirmRedirectPage(props: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  await redirectWithQuery("/reset/confirm", props.searchParams);
}
