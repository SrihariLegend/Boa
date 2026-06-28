#!/usr/bin/env python3
"""Read or patch simple Rust `const NAME: TYPE = VALUE;` declarations.

This is intentionally tiny and source-layout agnostic so tuning scripts do not
need to know whether search constants live in `src/search.rs` or in a module
like `src/search/constants.rs`.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="Rust source file to patch")
    parser.add_argument("name", help="Constant name, e.g. FFP_BASE_MARGIN")
    parser.add_argument("value", nargs="?", help="New value. Omit to print current value")
    args = parser.parse_args()

    text = args.source.read_text()
    pattern = re.compile(
        rf"(?P<prefix>(?:pub(?:\([^)]*\))?\s+)?const\s+{re.escape(args.name)}\s*:\s*[^=]+?=\s*)"
        rf"(?P<value>[^;]+)"
        rf"(?P<suffix>;)",
        re.MULTILINE,
    )
    match = pattern.search(text)
    if not match:
        raise SystemExit(f"could not find const {args.name} in {args.source}")

    if args.value is None:
        print(match.group("value").strip())
        return

    patched = pattern.sub(
        lambda m: f'{m.group("prefix")}{args.value}{m.group("suffix")}',
        text,
        count=1,
    )
    args.source.write_text(patched)


if __name__ == "__main__":
    main()
