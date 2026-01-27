"""
Context Injector: Convert llmcc output to agent-friendly context.

This module runs llmcc on a repository and converts the output
into a format suitable for injection into an AI agent's prompt.
"""

import subprocess
import tempfile
import re
from pathlib import Path
from typing import Optional
import json


class LlmccContextInjector:
    """Generate and format llmcc context for AI agents."""
    
    def __init__(
        self,
        llmcc_binary: str = "./target/release/llmcc",
        depth: int = 3,
        pagerank_top_k: int = 200,
    ):
        self.llmcc_binary = Path(llmcc_binary).resolve()
        self.depth = depth
        self.pagerank_top_k = pagerank_top_k
        
        if not self.llmcc_binary.exists():
            raise FileNotFoundError(f"llmcc binary not found: {self.llmcc_binary}")
    
    def generate_context(self, repo_path: str) -> dict:
        """
        Run llmcc on a repository and return structured context.
        
        Returns:
            dict with keys:
                - dot: Raw DOT graph output
                - markdown: Human-readable markdown summary
                - nodes: List of important nodes
                - stats: Timing and size statistics
        """
        repo_path = Path(repo_path).resolve()
        
        if not repo_path.exists():
            raise FileNotFoundError(f"Repository not found: {repo_path}")
        
        with tempfile.NamedTemporaryFile(suffix='.dot', delete=False) as f:
            dot_file = f.name
        
        try:
            # Run llmcc
            result = subprocess.run(
                [
                    str(self.llmcc_binary),
                    '-d', str(repo_path),
                    '--graph',
                    '--depth', str(self.depth),
                    '--pagerank-top-k', str(self.pagerank_top_k),
                    '-o', dot_file,
                ],
                capture_output=True,
                text=True,
                timeout=120,
                env={'RUST_LOG': 'info'},
            )
            
            # Parse timing from stderr
            stats = self._parse_stats(result.stderr)
            
            # Read DOT output
            with open(dot_file) as f:
                dot_content = f.read()
            
            # Parse nodes from DOT
            nodes = self._parse_nodes(dot_content)
            
            # Generate markdown summary
            markdown = self._generate_markdown(repo_path.name, nodes, stats)
            
            return {
                'dot': dot_content,
                'markdown': markdown,
                'nodes': nodes,
                'stats': stats,
            }
            
        finally:
            Path(dot_file).unlink(missing_ok=True)
    
    def _parse_stats(self, stderr: str) -> dict:
        """Parse timing statistics from llmcc output."""
        stats = {}
        
        patterns = {
            'files': r'Parsing total (\d+) files',
            'total_time': r'Total time: ([\d.]+)s',
            'parse_time': r'Parsing & tree-sitter: ([\d.]+)s',
            'binding_time': r'Symbol binding: ([\d.]+)s',
        }
        
        for key, pattern in patterns.items():
            match = re.search(pattern, stderr)
            if match:
                value = match.group(1)
                stats[key] = int(value) if key == 'files' else float(value)
        
        return stats
    
    def _parse_nodes(self, dot_content: str) -> list:
        """Extract node information from DOT graph."""
        nodes = []
        
        # Match node definitions: n123[label="crate::module::function"]
        pattern = r'n\d+\[label="([^"]+)"'
        
        for match in re.finditer(pattern, dot_content):
            label = match.group(1)
            # Parse the label to extract type and name
            parts = label.split('::')
            nodes.append({
                'path': label,
                'name': parts[-1] if parts else label,
                'module': '::'.join(parts[:-1]) if len(parts) > 1 else '',
            })
        
        return nodes
    
    def _generate_markdown(self, repo_name: str, nodes: list, stats: dict) -> str:
        """Generate a markdown summary for agent consumption."""
        lines = [
            f"## Architecture Overview: {repo_name}",
            "",
            f"This codebase contains {stats.get('files', '?')} files. "
            f"Here are the {len(nodes)} most important code elements "
            f"(ranked by PageRank centrality):",
            "",
        ]
        
        # Group nodes by module
        modules = {}
        for node in nodes:
            mod = node['module'] or '(root)'
            if mod not in modules:
                modules[mod] = []
            modules[mod].append(node['name'])
        
        # Sort modules by number of important nodes (descending)
        sorted_modules = sorted(modules.items(), key=lambda x: -len(x[1]))
        
        for mod, names in sorted_modules[:20]:  # Top 20 modules
            lines.append(f"### `{mod}`")
            for name in names[:10]:  # Top 10 items per module
                lines.append(f"- `{name}`")
            if len(names) > 10:
                lines.append(f"- ... and {len(names) - 10} more")
            lines.append("")
        
        lines.extend([
            "---",
            "",
            "Use this architecture map to navigate the codebase efficiently. ",
            "Start by identifying which module is relevant to the issue, ",
            "then explore the specific functions/types listed.",
        ])
        
        return '\n'.join(lines)
    
    def get_system_prompt_addition(self, repo_path: str) -> str:
        """
        Generate a system prompt addition with llmcc context.
        
        This is the main interface for integrating with agents.
        """
        try:
            context = self.generate_context(repo_path)
            return f"""
<codebase_architecture>
{context['markdown']}
</codebase_architecture>

Note: This architecture map was generated by analyzing the codebase's 
call graph and ranking code elements by importance. Use it to quickly
identify where to look for the relevant code.
"""
        except Exception as e:
            return f"<!-- llmcc context generation failed: {e} -->"


# Convenience function for quick context generation
def get_llmcc_context(repo_path: str, **kwargs) -> str:
    """Generate llmcc context for a repository."""
    injector = LlmccContextInjector(**kwargs)
    return injector.get_system_prompt_addition(repo_path)


if __name__ == "__main__":
    import sys
    
    if len(sys.argv) < 2:
        print("Usage: python context_injector.py <repo_path>")
        sys.exit(1)
    
    repo_path = sys.argv[1]
    print(get_llmcc_context(repo_path))
