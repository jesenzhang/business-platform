# Workspace Relocation: Staging Script Changes

The companion patch records six path-only changes made to the staging copy:

- `sample-resources.ps1` and `sample-resources2.ps1` write evidence relative to the script directory.
- `tools/capture-authcode.py` and `tools/prod-e2e-acceptance.py` resolve the staging root from their script location.
- `start-services-dev.sh` resolves both the staging root and adjacent repository from its own location.
- `ci-contract-battery.sh` resolves the repository and failure-evidence directory relative to its script location.

The patch contains no service logs, database files, secrets, or other runtime data. The main repository and staging directory are expected to be adjacent. From the staging root, locate the patch in the neighboring repository (or set `patch_file` to its absolute path if the directories are elsewhere):

```sh
patch_file="../business-platform/docs/operations/workspace-relocation/staging-script-changes.patch"
git apply --check --unidiff-zero "$patch_file"
git apply --unidiff-zero "$patch_file"
```

If the check fails, stop and do not overwrite or alter existing changes. On the already-relocated staging copy, verify the inverse without changing files:

```sh
git apply --reverse --check --unidiff-zero "$patch_file"
```

If a rollback is intentionally needed after review, the reverse command is `git apply --reverse --unidiff-zero "$patch_file"`; it is documented here and has not been executed.

These path changes are not evidence that services, production behavior, or historical gates were tested or accepted.
