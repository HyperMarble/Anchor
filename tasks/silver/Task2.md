# Task 2 — Preserve file line endings in replace_range edits

## Status

Good, but probably easier than Task 1. Make the tests precise and byte-based.

## Core bug

`replace_range` reads a file, splits using `.lines()`, and rebuilds content using `'\n'`. That normalizes CRLF files to LF even when only a small range is replaced.

## Suggested task title

```markdown
# Preserve file line endings in replace_range edits
```

## instruction.md

```markdown
# Preserve file line endings in replace_range edits

Anchor’s `replace_range` write operation should make the requested line-range edit without changing unrelated formatting in the rest of the file.

At the moment, replacing a range in a file that uses CRLF line endings can rewrite the file using LF line endings. That creates noisy diffs outside the requested edit and changes bytes that the user did not ask Anchor to touch.

## Expected behavior

`replace_range` should preserve the file’s existing line-ending style when rebuilding the edited file.

If the input file uses CRLF line endings, the output should keep CRLF line endings outside and around the replaced range.

If the input file uses LF line endings, the output should continue using LF line endings.

If replacement content contains multiple lines, those lines should be written using the same line-ending style as the file being edited.

If replacement content does not include a trailing newline, `replace_range` may add the appropriate line separator needed to keep the file structurally valid, but it should use the same line-ending style as the original file.

The existing behavior for valid LF files should not regress.

## Cases that must be handled

A CRLF file with a middle range replaced should remain CRLF after the edit.

A CRLF file with the first line replaced should keep CRLF line endings.

A CRLF file with the final line replaced should keep CRLF line endings and preserve the original trailing-newline behavior.

An LF file should continue to be written with LF line endings.

Replacement content that itself contains multiple lines should be normalized to the file’s existing line-ending style.

Replacement content without a trailing newline should not cause the rest of the file to switch line-ending style.

## Constraints

Do not change the public signature of `replace_range`.

Do not change the semantics of line numbers. They should remain 1-indexed and inclusive.

Do not introduce platform-dependent behavior. The same input bytes should produce the same output bytes on every operating system.

Do not modify unrelated write operations unless needed to share a small helper.

The behavior should be verified by reading the resulting file bytes or string content, not by checking source code text.
```

## reference_plan.md

```markdown
# Reference plan

## Root cause

`replace_range` uses `original.lines()` and then rebuilds output with `'\n'`. `.lines()` strips line terminators, so the original distinction between LF and CRLF is lost during reconstruction.

## Intended fix

Detect the existing line-ending style before splitting the file. Rebuild the output using the detected separator, and normalize replacement content to that same separator before insertion.

Preserve the existing trailing-newline behavior.

## Test plan

Use temporary files and write exact byte strings. After calling `replace_range`, read the resulting file bytes/string and assert the full output.

Tests should include CRLF middle, first-line, last-line, LF baseline, multiline replacement normalization, and missing trailing newline behavior.

## Difficulty notes

The task is fair because the instruction describes a user-visible write behavior. The agent must reason about how `.lines()` affects newline reconstruction and preserve existing write semantics.
```

## Correct test split

### pass_to_pass

These likely already pass at the base commit:

```json
[
  "replace_range_keeps_lf_files_lf"
]
```

Depending on exact expected bytes, this may also already pass:

```json
[
  "replace_range_preserves_missing_trailing_newline_style_for_lf"
]
```

Only include it in `pass_to_pass` after confirming locally.

### fail_to_pass

These should fail at the base commit:

```json
[
  "replace_range_preserves_crlf_for_middle_range",
  "replace_range_preserves_crlf_when_replacing_first_line",
  "replace_range_preserves_crlf_when_replacing_last_line",
  "replace_range_normalizes_multiline_replacement_to_existing_crlf",
  "replace_range_preserves_crlf_without_trailing_newline"
]
```

## Suggested selected test file

```text
tests/integration_replace_range_line_endings.rs
```

## Testing notes

Read and compare exact strings or bytes. Example expected output shape:

```text
"line 1\r\nnew A\r\nnew B\r\nline 5\r\n"
```

Make sure the test checks that `\r\n` remains present and that there are no unwanted bare `\n` separators.

## Risk review

Medium-low risk.

This task may be easier than Task 1, so do not make it just one CRLF test. Keep the multiline replacement and trailing-newline cases.

---
