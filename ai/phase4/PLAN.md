# Phase 4 — GitHub Integration

- Status: Not started
- Depends on: Phase 1 (provider trait), Phase 2 (repositories)

## Objectives

Implement the GitHub provider: API client, paginated repository
discovery, account/installation metadata, permission validation.

## Requirements

- `crates/github-provider` implementing `GitProvider`.
- Correct pagination (never assume the first page is complete);
  streaming/batched processing for thousands of repos.
- Repository metadata: visibility, default branch, size, updated_at,
  archived/fork flags, clone URL (credential-free).
- Rate-limit awareness (headers, backoff) and permission validation.
- Discovery writes to the `repositories` table via Phase 2 repositories.

## Implementation tasks

- [ ] GitHub API client (reqwest, auth header injection, retries)
- [ ] Repository discovery with pagination + cursors
- [ ] Installation metadata + repository permission checks
- [ ] Account metadata (username, avatar)
- [ ] Mocked-API integration tests (no real credentials)

## Dependencies

Phase 1 trait; Phase 2 DB repositories; Phase 5 for real credentials
(discovery can be tested with mocked tokens).

## Acceptance criteria

- Discovery of a multi-page mocked account yields all repos, in order,
  with correct metadata; rate-limit responses back off and resume.
- No repo is identified by name alone.

## Testing

- Mocked GitHub API (wiremock or local server) for pagination, errors,
  rate limits; unit tests for mapping.

## Known issues

None yet.

## Completion status

Not started.
