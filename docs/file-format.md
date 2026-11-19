# The set file format

A set (a project) saves as RON text, by convention with the `.ron` extension. The first lines of the file name its format:

```ron
(
    version: 12,
    project: ( ... ),
)
```

`FORMAT_VERSION` in `crates/qtable-engine/src/file.rs` holds the number the current build writes. The header is read before the body, so a build that doesn't know a format reports "unsupported project format version N" and never tries to read it.

## What a build reads and writes

- A release reads every older format. Each older format has a frozen reader in `crates/qtable-engine/src/file/`, and a set walks forward one format at a time until it reaches the current model.
- No build reads a newer format than its own `FORMAT_VERSION`.
- No build writes an older format. Saving a set always writes the current one, so opening an old set and saving it upgrades it in place. Keep a copy first if you may want to open it with an older build.
- Format 12 is the first public format. Formats 1 to 11 still load, but no public release ever wrote them.

## Unknown fields are errors

Every stored type rejects a field it doesn't know (`#[serde(deny_unknown_fields)]`). A hand-edited typo such as `delay:` for `delay_ms:` fails to load with the field named, rather than loading as the default.

Spellings the format has accepted stay accepted (for example `stop_button` for a slot's `has_stop_button`).

## Changing the format

- **Any new stored field or enum variant bumps `FORMAT_VERSION`**, even one with a default that the writer omits. An older build can't tell a new field from a typo: it either rejects the file or, worse, drops the field on its next save.
- Add the new format's frozen reader and its fixture in the same change. A fixture written by the previous format's writer, before the bump, shows that old sets still load and save as they did.
- Add every new stored type to `STORED_TYPES` in `crates/qtable-engine/tests/strict_stored_fields.rs`. That test plants an unknown field in each struct of every fixture and example and expects the load to refuse it.
- Note the bump in CHANGELOG.md. A release's heading records its format (see [releasing.md](releasing.md)).
