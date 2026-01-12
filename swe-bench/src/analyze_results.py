"""
Analyze SWE-bench experiment results.

Compares baseline vs llmcc-enhanced agent performance.
"""

import argparse
import json
from pathlib import Path
from typing import Optional
import pandas as pd


def load_results(results_dir: Path) -> list:
    """Load all result JSON files from a directory."""
    results = []
    
    for json_file in results_dir.glob('**/*.json'):
        if json_file.name.startswith('summary'):
            continue
        try:
            with open(json_file) as f:
                results.append(json.load(f))
        except Exception as e:
            print(f"Failed to load {json_file}: {e}")
    
    return results


def calculate_metrics(results: list) -> dict:
    """Calculate aggregate metrics from results."""
    if not results:
        return {'count': 0}
    
    successful = [r for r in results if r.get('success')]
    
    total_input_tokens = sum(
        r.get('usage', {}).get('input_tokens', 0) 
        for r in successful
    )
    total_output_tokens = sum(
        r.get('usage', {}).get('output_tokens', 0) 
        for r in successful
    )
    
    return {
        'count': len(results),
        'successful': len(successful),
        'success_rate': len(successful) / len(results) if results else 0,
        'total_input_tokens': total_input_tokens,
        'total_output_tokens': total_output_tokens,
        'avg_input_tokens': total_input_tokens / len(successful) if successful else 0,
        'avg_output_tokens': total_output_tokens / len(successful) if successful else 0,
    }


def compare_results(baseline_dir: Path, llmcc_dir: Path) -> dict:
    """Compare baseline vs llmcc results."""
    baseline_results = load_results(baseline_dir)
    llmcc_results = load_results(llmcc_dir)
    
    baseline_metrics = calculate_metrics(baseline_results)
    llmcc_metrics = calculate_metrics(llmcc_results)
    
    comparison = {
        'baseline': baseline_metrics,
        'llmcc': llmcc_metrics,
    }
    
    # Calculate improvements
    if baseline_metrics['count'] > 0 and llmcc_metrics['count'] > 0:
        comparison['improvements'] = {
            'success_rate_diff': (
                llmcc_metrics['success_rate'] - baseline_metrics['success_rate']
            ),
            'token_efficiency': (
                (baseline_metrics['avg_input_tokens'] - llmcc_metrics['avg_input_tokens'])
                / baseline_metrics['avg_input_tokens']
                if baseline_metrics['avg_input_tokens'] > 0 else 0
            ),
        }
    
    return comparison


def print_report(comparison: dict):
    """Print a formatted comparison report."""
    print("\n" + "=" * 60)
    print("SWE-BENCH EXPERIMENT RESULTS")
    print("=" * 60)
    
    print("\n## Baseline (no llmcc context)")
    baseline = comparison['baseline']
    print(f"  Tasks: {baseline['count']}")
    print(f"  Successful API calls: {baseline['successful']}")
    print(f"  Avg input tokens: {baseline['avg_input_tokens']:.0f}")
    print(f"  Avg output tokens: {baseline['avg_output_tokens']:.0f}")
    
    print("\n## With llmcc context")
    llmcc = comparison['llmcc']
    print(f"  Tasks: {llmcc['count']}")
    print(f"  Successful API calls: {llmcc['successful']}")
    print(f"  Avg input tokens: {llmcc['avg_input_tokens']:.0f}")
    print(f"  Avg output tokens: {llmcc['avg_output_tokens']:.0f}")
    
    if 'improvements' in comparison:
        print("\n## Comparison")
        improvements = comparison['improvements']
        print(f"  Token efficiency change: {improvements['token_efficiency']*100:+.1f}%")
    
    print("\n" + "=" * 60)
    print("NOTE: These are API call metrics only.")
    print("For actual resolution rates, run the SWE-bench evaluator:")
    print("  python -m swebench.harness.run_evaluation ...")
    print("=" * 60)


def generate_markdown_report(comparison: dict, output_file: Path):
    """Generate a markdown report."""
    lines = [
        "# SWE-bench Experiment Results",
        "",
        f"Generated: {pd.Timestamp.now().isoformat()}",
        "",
        "## Summary",
        "",
        "| Metric | Baseline | With llmcc | Difference |",
        "|--------|----------|------------|------------|",
    ]
    
    baseline = comparison['baseline']
    llmcc = comparison['llmcc']
    
    lines.append(
        f"| Tasks | {baseline['count']} | {llmcc['count']} | - |"
    )
    lines.append(
        f"| Avg Input Tokens | {baseline['avg_input_tokens']:.0f} | "
        f"{llmcc['avg_input_tokens']:.0f} | "
        f"{llmcc['avg_input_tokens'] - baseline['avg_input_tokens']:+.0f} |"
    )
    lines.append(
        f"| Avg Output Tokens | {baseline['avg_output_tokens']:.0f} | "
        f"{llmcc['avg_output_tokens']:.0f} | "
        f"{llmcc['avg_output_tokens'] - baseline['avg_output_tokens']:+.0f} |"
    )
    
    lines.extend([
        "",
        "## Next Steps",
        "",
        "1. Run SWE-bench evaluator to get actual resolution rates",
        "2. Compare pass@1 metrics between baseline and llmcc",
        "3. Analyze failure cases to identify patterns",
    ])
    
    with open(output_file, 'w') as f:
        f.write('\n'.join(lines))
    
    print(f"Report saved to: {output_file}")


def main():
    parser = argparse.ArgumentParser(
        description='Analyze SWE-bench experiment results'
    )
    parser.add_argument(
        '--baseline', '-b',
        required=True,
        help='Path to baseline results directory'
    )
    parser.add_argument(
        '--llmcc', '-l',
        required=True,
        help='Path to llmcc results directory'
    )
    parser.add_argument(
        '--output', '-o',
        default=None,
        help='Output markdown report file'
    )
    
    args = parser.parse_args()
    
    comparison = compare_results(
        Path(args.baseline),
        Path(args.llmcc)
    )
    
    print_report(comparison)
    
    if args.output:
        generate_markdown_report(comparison, Path(args.output))


if __name__ == '__main__':
    main()
