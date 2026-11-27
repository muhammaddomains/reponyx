import { redirectWithQuery } from "../redirect";

export default async function AuthRegisterRedirectPage(props: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  await redirectWithQuery("/register", props.searchParams);
}
