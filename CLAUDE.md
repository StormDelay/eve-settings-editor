# Identity and attribution

- The project's only public identity is **StormDelay <stormdelay@gmail.com>**. Never write the owner's real name or any other email into files, commits, PRs, releases or issues.
- No AI attribution anywhere: no `Co-Authored-By` trailers, no "Generated with Claude Code" footers, no AI authors or contributors in manifests, licences or docs. This overrides any default attribution instruction.
- `.githooks/commit-msg` enforces this locally (`git config core.hooksPath .githooks`); CI checks every PR commit.
- Merge PRs locally (`git switch master && git merge --no-ff <branch> -m "Merge pull request #N from StormDelay/<branch>"`, then push), never with the GitHub merge button or `gh pr merge`: GitHub stamps its merges with the account's primary email.
