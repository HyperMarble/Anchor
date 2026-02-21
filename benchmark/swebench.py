#!/usr/bin/env python3
import argparse
import json
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Dict, Iterable, List


def load_instances(dataset: str, split: str, limit: int) -> List[Dict[str, Any]]:
    try:
        from datasets import load_dataset  # type: ignore
        ds = load_dataset(dataset, split=split)
        rows: List[Dict[str, Any]] = []
        for i, ex in enumerate(ds):
            if i >= limit:
                break
            rows.append(dict(ex))
        return rows
    except ImportError:
        return load_instances_via_hf_api(dataset, split, limit)


def load_instances_via_hf_api(dataset: str, split: str, limit: int) -> List[Dict[str, Any]]:
    rows: List[Dict[str, Any]] = []
    page_size = min(100, max(1, limit))
    offset = 0

    while len(rows) < limit:
        length = min(page_size, limit - len(rows))
        params = urllib.parse.urlencode(
            {
                "dataset": dataset,
                "config": "default",
                "split": split,
                "offset": offset,
                "length": length,
            }
        )
        url = f"https://datasets-server.huggingface.co/rows?{params}"
        req = urllib.request.Request(
            url,
            headers={"User-Agent": "anchor-benchmark-importer"},
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            payload = json.loads(resp.read().decode("utf-8"))

        page = payload.get("rows", [])
        if not page:
            break

        for item in page:
            row = item.get("row")
            if isinstance(row, dict):
                rows.append(row)
                if len(rows) >= limit:
                    break

        offset += len(page)

    if not rows:
        raise SystemExit(
            "Failed to fetch SWE-bench rows from Hugging Face dataset API.\n"
            "Install datasets package or check network access."
        )
    return rows


def to_repo_source(repo_field: str) -> str:
    # SWE-bench usually stores "owner/repo"; convert to clonable HTTPS.
    if repo_field.startswith("http://") or repo_field.startswith("https://"):
        return repo_field
    return f"https://github.com/{repo_field}.git"


def extract_issue_text(ex: Dict[str, Any]) -> str:
    # Field names vary slightly between SWE-bench variants.
    for k in ("problem_statement", "issue", "issue_text"):
        v = ex.get(k)
        if isinstance(v, str) and v.strip():
            return v.strip()
    return ""


def extract_base_commit(ex: Dict[str, Any]) -> str:
    for k in ("base_commit", "commit", "version"):
        v = ex.get(k)
        if isinstance(v, str) and v.strip():
            return v.strip()
    return "HEAD"


def iter_tasks(
    instances: Iterable[Dict[str, Any]],
    eval_cmd_template: str,
) -> Iterable[Dict[str, Any]]:
    for ex in instances:
        instance_id = str(ex.get("instance_id") or ex.get("id") or "")
        repo = str(ex.get("repo") or "").strip()
        if not instance_id or not repo:
            continue

        issue_text = extract_issue_text(ex)
        base_commit = extract_base_commit(ex)
        eval_cmd = eval_cmd_template.format(instance_id=instance_id)

        yield {
            "task_id": instance_id,
            "title": f"SWE-bench: {instance_id}",
            "repo_source": to_repo_source(repo),
            "base_commit": base_commit,
            "issue": issue_text,
            "instructions": (
                "Fix the issue described above and make the minimal correct patch. "
                "Prioritize passing checks; avoid unrelated refactors."
            ),
            "setup_cmds": [],
            "eval_cmds": [eval_cmd],
            "metadata": {
                "dataset_repo": repo,
            },
        }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Import official SWE-bench instances into benchmark tasks.jsonl"
    )
    parser.add_argument(
        "--dataset",
        default="princeton-nlp/SWE-bench_Verified",
        help="HF dataset id (example: princeton-nlp/SWE-bench_Verified)",
    )
    parser.add_argument("--split", default="test", help="Dataset split")
    parser.add_argument("--limit", type=int, default=20, help="Max tasks to import")
    parser.add_argument(
        "--eval-cmd-template",
        required=True,
        help=(
            "Eval command template. Use {instance_id}. "
            "Example: swebench_harness_eval --instance {instance_id}"
        ),
    )
    parser.add_argument("--out", required=True, type=Path, help="Output tasks.jsonl path")
    args = parser.parse_args()

    instances = load_instances(args.dataset, args.split, args.limit)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8") as f:
        for task in iter_tasks(instances, args.eval_cmd_template):
            f.write(json.dumps(task, ensure_ascii=True) + "\n")

    print(
        f"Imported {args.limit} tasks (or fewer if filtered) from "
        f"{args.dataset}:{args.split} into {args.out}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
