"""
SWE-bench Experiment Runner

Runs SWE-bench evaluation with and without llmcc context injection.
"""

import argparse
import json
import os
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional
import yaml
from tqdm import tqdm

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from context_injector import LlmccContextInjector


def load_config(config_path: str) -> dict:
    """Load experiment configuration from YAML file."""
    with open(config_path) as f:
        return yaml.safe_load(f)


def load_rust_tasks(config: dict) -> list:
    """Load SWE-bench Multilingual Rust tasks."""
    from datasets import load_dataset
    
    dataset_name = config['dataset']['name']
    split = config['dataset']['split']
    target_repos = config['dataset']['repos']
    
    print(f"Loading dataset: {dataset_name}")
    ds = load_dataset(dataset_name, split=split)
    
    # Filter for target repositories
    tasks = []
    for item in ds:
        # Handle repo format (e.g., "tokio-rs__tokio" or "tokio-rs/tokio")
        repo = item['repo'].replace('__', '/')
        if repo in target_repos:
            tasks.append({
                'instance_id': item['instance_id'],
                'repo': repo,
                'base_commit': item['base_commit'],
                'problem_statement': item['problem_statement'],
                'hints_text': item.get('hints_text', ''),
                'test_patch': item['test_patch'],
                'patch': item['patch'],  # Gold patch for reference
            })
    
    print(f"Found {len(tasks)} tasks from {len(set(t['repo'] for t in tasks))} repos")
    return tasks


def clone_repo(repo: str, commit: str, cache_dir: Path) -> Path:
    """Clone a repository at a specific commit."""
    import subprocess
    
    repo_name = repo.replace('/', '__')
    repo_dir = cache_dir / repo_name / commit[:8]
    
    if repo_dir.exists():
        return repo_dir
    
    repo_dir.parent.mkdir(parents=True, exist_ok=True)
    
    # Clone
    subprocess.run([
        'git', 'clone', '--depth', '1',
        f'https://github.com/{repo}.git',
        str(repo_dir)
    ], check=True, capture_output=True)
    
    # Checkout specific commit (fetch it first)
    subprocess.run(
        ['git', 'fetch', '--depth', '1', 'origin', commit],
        cwd=repo_dir, check=True, capture_output=True
    )
    subprocess.run(
        ['git', 'checkout', commit],
        cwd=repo_dir, check=True, capture_output=True
    )
    
    return repo_dir


def run_agent_on_task(
    task: dict,
    config: dict,
    llmcc_context: Optional[str] = None,
    output_dir: Path = None,
) -> dict:
    """
    Run the AI agent on a single task.
    
    This is a simplified version - in practice, you'd integrate with
    mini-SWE-agent or a similar harness.
    """
    import anthropic
    
    client = anthropic.Anthropic()
    
    # Build the prompt
    system_prompt = """You are an expert software engineer. Your task is to fix a bug 
or implement a feature based on the GitHub issue provided.

You have access to the repository and can:
1. Read files
2. Search for code
3. Make edits to fix the issue

Provide your solution as a unified diff patch."""

    if llmcc_context:
        system_prompt = llmcc_context + "\n\n" + system_prompt
    
    user_prompt = f"""## Issue

{task['problem_statement']}

## Repository: {task['repo']}

Please analyze the issue and provide a patch to fix it."""

    # Call the model
    model = config['model']['name']
    
    try:
        response = client.messages.create(
            model=model,
            max_tokens=config['model']['max_tokens'],
            temperature=config['model']['temperature'],
            system=system_prompt,
            messages=[{"role": "user", "content": user_prompt}],
        )
        
        result = {
            'instance_id': task['instance_id'],
            'model': model,
            'with_llmcc': llmcc_context is not None,
            'response': response.content[0].text,
            'usage': {
                'input_tokens': response.usage.input_tokens,
                'output_tokens': response.usage.output_tokens,
            },
            'success': True,
            'error': None,
        }
        
    except Exception as e:
        result = {
            'instance_id': task['instance_id'],
            'model': model,
            'with_llmcc': llmcc_context is not None,
            'response': None,
            'usage': None,
            'success': False,
            'error': str(e),
        }
    
    # Save result
    if output_dir:
        output_file = output_dir / f"{task['instance_id']}.json"
        with open(output_file, 'w') as f:
            json.dump(result, f, indent=2)
    
    return result


def run_experiment(
    config_path: str,
    mode: str = 'both',  # 'baseline', 'llmcc', or 'both'
    max_tasks: Optional[int] = None,
    dry_run: bool = False,
):
    """Run the full experiment."""
    config = load_config(config_path)
    
    # Setup directories
    base_dir = Path(__file__).parent.parent
    cache_dir = base_dir / 'cache' / 'repos'
    results_dir = base_dir / config['output']['results_dir']
    logs_dir = base_dir / config['output']['logs_dir']
    
    cache_dir.mkdir(parents=True, exist_ok=True)
    results_dir.mkdir(parents=True, exist_ok=True)
    logs_dir.mkdir(parents=True, exist_ok=True)
    
    # Load tasks
    tasks = load_rust_tasks(config)
    if max_tasks:
        tasks = tasks[:max_tasks]
    
    print(f"Running experiment with {len(tasks)} tasks")
    print(f"Mode: {mode}")
    
    if dry_run:
        print("DRY RUN - not executing")
        for task in tasks:
            print(f"  - {task['instance_id']}: {task['repo']}")
        return
    
    # Initialize llmcc injector
    llmcc_binary = base_dir / config['llmcc']['binary']
    injector = LlmccContextInjector(
        llmcc_binary=str(llmcc_binary),
        depth=config['llmcc']['depth'],
        pagerank_top_k=config['llmcc']['pagerank_top_k'],
    )
    
    results = {
        'baseline': [],
        'llmcc': [],
    }
    
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    
    for task in tqdm(tasks, desc="Running tasks"):
        # Clone repo at the right commit
        try:
            repo_dir = clone_repo(
                task['repo'],
                task['base_commit'],
                cache_dir
            )
        except Exception as e:
            print(f"Failed to clone {task['repo']}: {e}")
            continue
        
        # Run baseline (no llmcc)
        if mode in ('baseline', 'both'):
            baseline_dir = results_dir / 'baseline' / timestamp
            baseline_dir.mkdir(parents=True, exist_ok=True)
            
            result = run_agent_on_task(
                task, config,
                llmcc_context=None,
                output_dir=baseline_dir
            )
            results['baseline'].append(result)
        
        # Run with llmcc
        if mode in ('llmcc', 'both'):
            llmcc_dir = results_dir / 'llmcc' / timestamp
            llmcc_dir.mkdir(parents=True, exist_ok=True)
            
            # Generate llmcc context
            try:
                context = injector.get_system_prompt_addition(str(repo_dir))
            except Exception as e:
                print(f"Failed to generate llmcc context for {task['repo']}: {e}")
                context = None
            
            result = run_agent_on_task(
                task, config,
                llmcc_context=context,
                output_dir=llmcc_dir
            )
            results['llmcc'].append(result)
    
    # Save summary
    summary = {
        'config': config,
        'timestamp': timestamp,
        'num_tasks': len(tasks),
        'baseline_results': len(results['baseline']),
        'llmcc_results': len(results['llmcc']),
    }
    
    summary_file = results_dir / f'summary_{timestamp}.json'
    with open(summary_file, 'w') as f:
        json.dump(summary, f, indent=2)
    
    print(f"\nResults saved to: {results_dir}")
    print(f"Summary: {summary_file}")


def main():
    parser = argparse.ArgumentParser(
        description='Run SWE-bench experiments with llmcc context injection'
    )
    parser.add_argument(
        '--config', '-c',
        required=True,
        help='Path to configuration YAML file'
    )
    parser.add_argument(
        '--mode', '-m',
        choices=['baseline', 'llmcc', 'both'],
        default='both',
        help='Experiment mode (default: both)'
    )
    parser.add_argument(
        '--max-tasks', '-n',
        type=int,
        default=None,
        help='Maximum number of tasks to run (for testing)'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='List tasks without running'
    )
    
    args = parser.parse_args()
    
    run_experiment(
        config_path=args.config,
        mode=args.mode,
        max_tasks=args.max_tasks,
        dry_run=args.dry_run,
    )


if __name__ == '__main__':
    main()
