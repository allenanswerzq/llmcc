"""Extract test cases from tree-sitter corpus .txt files into .llmcc format.

Usage: python scripts/extract_corpus.py <corpus_dir> <output_dir> [--lang rust]
"""
import os
import re
import sys
from pathlib import Path

BANNER = "=" * 79


def slugify(name: str) -> str:
    """Convert a test case name to a kebab-case slug."""
    slug = ""
    pending_dash = False
    for ch in name:
        if ch.isalnum():
            if pending_dash and slug:
                slug += "-"
            slug += ch.lower()
            pending_dash = False
        elif slug:
            pending_dash = True
    return slug or "case"


def parse_tree_sitter_corpus(path: Path) -> list[dict]:
    """Parse a tree-sitter corpus .txt file into test cases.

    Each case has:
      - name: the test case name
      - code: the source code (between banners and the --- separator)
    """
    content = path.read_text(encoding="utf-8")
    lines = content.split("\n")
    cases = []
    i = 0

    while i < len(lines):
        # Find opening banner
        if not lines[i].strip().startswith("=" * 20):
            i += 1
            continue

        i += 1
        # Read name (may span multiple lines, take first non-empty)
        name = ""
        while i < len(lines) and not lines[i].strip().startswith("=" * 20):
            if lines[i].strip() and not name:
                name = lines[i].strip()
            i += 1

        if i >= len(lines):
            break
        i += 1  # skip closing banner

        # Read code until separator (---...---)
        code_lines = []
        while i < len(lines):
            if lines[i].strip().startswith("-" * 20):
                i += 1
                break
            code_lines.append(lines[i])
            i += 1

        # Skip the s-expression (until next banner or EOF)
        while i < len(lines) and not lines[i].strip().startswith("=" * 20):
            i += 1

        # Trim trailing empty lines from code
        while code_lines and not code_lines[-1].strip():
            code_lines.pop()

        if name and code_lines:
            cases.append({"name": name, "code": "\n".join(code_lines)})

    return cases


def write_llmcc_file(cases: list[dict], output_path: Path, file_name: str = "src/lib.rs"):
    """Write test cases in .llmcc format."""
    output_path.parent.mkdir(parents=True, exist_ok=True)

    parts = []
    for case in cases:
        slug = slugify(case["name"])
        part = f"{BANNER}\n{slug}\n{BANNER}\n\n"
        part += f"--- file: {file_name} ---\n"
        part += case["code"] + "\n"
        part += "\n--- expect:block-graph ---\n"
        parts.append(part)

    content = "\n\n".join(parts) + "\n"
    output_path.write_text(content, encoding="utf-8", newline="\n")
    print(f"  {output_path.name}: {len(cases)} cases")


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <corpus_dir> <output_dir>")
        sys.exit(1)

    corpus_dir = Path(sys.argv[1])
    output_dir = Path(sys.argv[2])

    if not corpus_dir.is_dir():
        print(f"Error: {corpus_dir} is not a directory")
        sys.exit(1)

    txt_files = sorted(corpus_dir.glob("*.txt"))
    if not txt_files:
        print(f"No .txt files found in {corpus_dir}")
        sys.exit(1)

    print(f"Extracting from {corpus_dir} -> {output_dir}")
    total = 0

    for txt_file in txt_files:
        category = txt_file.stem  # e.g. "declarations"
        cases = parse_tree_sitter_corpus(txt_file)
        if not cases:
            continue

        # Write one .llmcc file per category
        out_path = output_dir / f"{category}.llmcc"
        write_llmcc_file(cases, out_path)
        total += len(cases)

    print(f"\nTotal: {total} test cases extracted")


if __name__ == "__main__":
    main()
