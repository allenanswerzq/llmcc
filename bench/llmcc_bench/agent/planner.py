"""
AI Planner: Leverages llmcc's multi-depth PageRank architecture graphs.

llmcc produces graphs at 4 depth levels:
  - Depth 0: Project level (multi-repo relationships)
  - Depth 1: Crate/Library level (ownership boundaries, public API)
  - Depth 2: Module level (subsystem structure)
  - Depth 3: File/Symbol level (implementation details)

Nodes are PageRank-ordered - the graph shows top ~200 most connected symbols.
This planner teaches the agent to navigate like a map: start high-level,
zoom into relevant areas, follow edges to trace dependencies.
"""

import json
import os
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

import anthropic


@dataclass
class GraphLevel:
    """Parsed information from one depth level of the graph."""
    depth: int
    depth_name: str  # "project", "crate", "module", "file"
    clusters: Dict[str, List[str]]  # cluster_name -> symbols/children
    nodes: Dict[str, dict]  # node_id -> {label, path, sym_ty, ...}
    edges: List[Tuple[str, str]]  # (from_node, to_node)


@dataclass
class ArchitectureMap:
    """Multi-depth architecture map from llmcc graphs."""

    levels: Dict[int, GraphLevel] = field(default_factory=dict)
    """Graphs by depth level (0-3)."""

    top_symbols: List[dict] = field(default_factory=list)
    """PageRank-ordered top symbols from depth 3."""

    crate_dependencies: Dict[str, Set[str]] = field(default_factory=dict)
    """Crate -> set of crates it depends on."""

    module_to_crate: Dict[str, str] = field(default_factory=dict)
    """Module name -> containing crate."""


@dataclass
class NavigationPlan:
    """Hierarchical navigation plan for exploring the codebase."""

    entry_points: List[dict]
    """Starting points: [{crate, module, file, symbol, reason}]"""

    exploration_paths: List[dict]
    """Paths to follow: [{from_symbol, to_symbol, edge_type, reason}]"""

    zoom_instructions: List[str]
    """How to zoom in/out if needed."""

    answer_strategy: str
    """How to answer once exploration is done."""

    direct_answer: Optional[str] = None
    """If graph directly answers the question, answer here."""


def parse_depth_3_graph(graph: str) -> Tuple[Dict[str, dict], List[Tuple[str, str]], Dict[str, List[str]]]:
    """
    Parse depth 3 (file-level) graph to extract nodes, edges, and clusters.

    Returns:
        - nodes: {node_id: {label, path, sym_ty}}
        - edges: [(from_id, to_id), ...]
        - clusters: {cluster_name: [node_ids]}
    """
    nodes = {}
    edges = []
    clusters = {}
    current_cluster = None
    cluster_stack = []

    for line in graph.split('\n'):
        line = line.strip()

        # Track cluster hierarchy (subgraph cluster_xxx { ... })
        if line.startswith('subgraph cluster_'):
            cluster_name = line.split('subgraph ')[1].split('{')[0].strip()
            cluster_stack.append(cluster_name)
            current_cluster = cluster_name
            if cluster_name not in clusters:
                clusters[cluster_name] = []
        elif line == '}' and cluster_stack:
            cluster_stack.pop()
            current_cluster = cluster_stack[-1] if cluster_stack else None

        # Parse nodes: n123[label="Name", path="...", sym_ty="..."]
        node_match = re.match(r'(n\d+)\[label="([^"]+)"', line)
        if node_match:
            node_id = node_match.group(1)
            label = node_match.group(2)

            # Extract path and sym_ty
            path_match = re.search(r'path="([^"]+)"', line)
            sym_ty_match = re.search(r'sym_ty="([^"]+)"', line)

            nodes[node_id] = {
                'label': label,
                'path': path_match.group(1) if path_match else None,
                'sym_ty': sym_ty_match.group(1) if sym_ty_match else None,
            }

            # Add to current cluster
            if current_cluster:
                clusters[current_cluster].append(node_id)

        # Parse edges: n1 -> n2
        edge_match = re.match(r'(n\d+)\s*->\s*(n\d+)', line)
        if edge_match:
            edges.append((edge_match.group(1), edge_match.group(2)))

    return nodes, edges, clusters


def parse_depth_1_graph(graph: str) -> Dict[str, Set[str]]:
    """Parse depth 1 (crate-level) graph for crate dependencies."""
    dependencies = {}

    for line in graph.split('\n'):
        line = line.strip()

        # Parse crate nodes
        crate_match = re.match(r'crate_(\w+)\[label="([^"]+)"', line)
        if crate_match:
            crate_name = crate_match.group(2)
            if crate_name not in dependencies:
                dependencies[crate_name] = set()

        # Parse edges: crate_x -> crate_y means x depends on y
        edge_match = re.match(r'crate_(\w+)\s*->\s*crate_(\w+)', line)
        if edge_match:
            from_crate = edge_match.group(1).replace('_', '-')
            to_crate = edge_match.group(2).replace('_', '-')
            if from_crate not in dependencies:
                dependencies[from_crate] = set()
            dependencies[from_crate].add(to_crate)

    return dependencies


def build_architecture_map(
    depth_1_graph: Optional[str],
    depth_2_graph: Optional[str],
    depth_3_graph: str,
) -> ArchitectureMap:
    """Build a unified architecture map from multiple depth graphs."""
    arch_map = ArchitectureMap()

    # Parse depth 3 (most detailed)
    nodes, edges, clusters = parse_depth_3_graph(depth_3_graph)

    # Extract PageRank-ordered symbols (order in file is PageRank order)
    for node_id, node_info in nodes.items():
        if node_info.get('path'):
            arch_map.top_symbols.append({
                'id': node_id,
                'name': node_info['label'],
                'path': node_info['path'],
                'type': node_info.get('sym_ty', 'Unknown'),
            })

    # Parse depth 1 for crate dependencies
    if depth_1_graph:
        arch_map.crate_dependencies = parse_depth_1_graph(depth_1_graph)

    return arch_map


def find_relevant_symbols(
    arch_map: ArchitectureMap,
    keywords: List[str],
) -> List[dict]:
    """Find symbols matching keywords, respecting PageRank order."""
    relevant = []
    keywords_lower = [k.lower() for k in keywords]

    for symbol in arch_map.top_symbols:
        name_lower = symbol['name'].lower()
        path_lower = symbol['path'].lower()

        for keyword in keywords_lower:
            if keyword in name_lower or keyword in path_lower:
                relevant.append(symbol)
                break

    return relevant


NAVIGATOR_SYSTEM_PROMPT = """You are a code architecture navigator using an llmcc PageRank architecture graph.

## Graph Structure
- Nodes are PageRank-ordered (early = more important/central)
- Depth 1 = crates/libraries, Depth 2 = modules, Depth 3 = files/symbols
- Edges show dependencies: A -> B means A uses/depends on B
- path="file:line" gives exact location

## Navigation Principles

1. START AT THE RIGHT ZOOM LEVEL
   - "What crates depend on X?" → depth 1 (crate graph)
   - "What modules in X?" → depth 2 (module graph)
   - "How does X work?" → depth 3 (symbol graph)

2. FOLLOW PAGERANK
   - Top symbols are most central/important
   - If task mentions a core concept, the top 20 symbols likely contain it
   - Less central code is filtered out of the top 200

3. READ EDGES AS A MAP
   - To find "what uses X": look for edges pointing TO X
   - To find "what X depends on": look for edges FROM X
   - Edge chains = call paths / data flow

4. ZOOM IN/OUT AS NEEDED
   - Can't find symbol? It's below PageRank threshold → zoom into that folder
   - Need bigger picture? Look at crate/module level first

## Response Format

For LOCATION/DEPENDENCY questions (what uses X, where is X):
- Answer DIRECTLY from the graph if possible
- Set direct_answer with the answer

For IMPLEMENTATION questions (how does X work):
- Identify entry points (1-3 key symbols)
- Trace exploration paths via edges
- Provide zoom instructions if agent needs more detail

Return JSON:
{
    "direct_answer": "Answer here if graph directly answers the question, else null",
    "entry_points": [
        {"crate": "...", "module": "...", "file": "...", "symbol": "...", "reason": "why start here"}
    ],
    "exploration_paths": [
        {"from": "SymbolA", "to": "SymbolB", "edge_type": "calls/uses/implements", "reason": "what this shows"}
    ],
    "zoom_instructions": [
        "If X not found, run llmcc on subfolder Y",
        "For more detail on Z, look at the impl block"
    ],
    "answer_strategy": "How to synthesize final answer"
}"""


def create_navigator_prompt(
    task: str,
    depth_1_graph: Optional[str],
    depth_3_graph: str,
    workspace_path: str,
) -> str:
    """Create prompt for the navigator with multi-depth context."""

    # Parse for summary
    nodes, edges, clusters = parse_depth_3_graph(depth_3_graph)

    # Top 30 symbols by PageRank (order in dict is PageRank order)
    top_symbols = []
    for node_id, info in list(nodes.items())[:30]:
        if info.get('path'):
            file_path = info['path'].split(':')[0]
            # Make path relative to workspace
            if workspace_path in file_path:
                file_path = file_path.replace(workspace_path + '/', '')
            top_symbols.append(f"  {info['label']} ({info.get('sym_ty', '?')}) @ {file_path}")

    # Crate structure from depth 1
    crate_section = ""
    if depth_1_graph:
        deps = parse_depth_1_graph(depth_1_graph)
        crate_lines = []
        for crate, crate_deps in sorted(deps.items()):
            if crate_deps:
                crate_lines.append(f"  {crate} → {', '.join(sorted(crate_deps))}")
            else:
                crate_lines.append(f"  {crate}")
        crate_section = f"""
## Crate Dependencies (Depth 1)
{chr(10).join(crate_lines[:20])}
"""

    return f"""## Task
{task}

## Workspace
{workspace_path}
{crate_section}
## Top 30 PageRank Symbols (Depth 3)
{chr(10).join(top_symbols)}

## Full Depth 3 Graph (DOT format)
```dot
{depth_3_graph[:20000]}
```
{"[... truncated ...]" if len(depth_3_graph) > 20000 else ""}

Based on this architecture map, provide a navigation plan to complete the task.
If the graph directly answers the question, provide the answer in direct_answer."""


async def generate_plan(
    task_description: str,
    depth_3_graph: str,
    workspace_path: Path,
    depth_1_graph: Optional[str] = None,
    depth_2_graph: Optional[str] = None,
    model: str = "claude-3-5-haiku-20241022",
) -> NavigationPlan:
    """
    Generate a navigation plan using architecture graphs.

    Uses copilot-bridge (localhost:5168) by default, same as ClaudeAgentRunner.
    Falls back to direct Anthropic API if ANTHROPIC_API_KEY is set.

    Args:
        task_description: The task to complete.
        depth_3_graph: File/symbol level graph (required).
        workspace_path: Path to the workspace.
        depth_1_graph: Crate level graph (optional).
        depth_2_graph: Module level graph (optional).
        model: Model to use for planning.

    Returns:
        NavigationPlan with hierarchical exploration instructions.
    """
    # Use copilot-bridge by default (same as ClaudeAgentRunner)
    base_url = os.environ.get("ANTHROPIC_BASE_URL", "http://localhost:5168")
    api_key = os.environ.get("ANTHROPIC_API_KEY", "sk-copilot-bridge")

    client = anthropic.Anthropic(
        api_key=api_key,
        base_url=base_url,
    )

    prompt = create_navigator_prompt(
        task_description,
        depth_1_graph,
        depth_3_graph,
        str(workspace_path),
    )

    response = client.messages.create(
        model=model,
        max_tokens=2048,
        system=NAVIGATOR_SYSTEM_PROMPT,
        messages=[{"role": "user", "content": prompt}],
    )

    response_text = response.content[0].text

    # Parse JSON response
    try:
        if "```json" in response_text:
            json_match = re.search(r'```json\s*(.*?)\s*```', response_text, re.DOTALL)
            if json_match:
                response_text = json_match.group(1)
        elif "```" in response_text:
            json_match = re.search(r'```\s*(.*?)\s*```', response_text, re.DOTALL)
            if json_match:
                response_text = json_match.group(1)

        plan_data = json.loads(response_text)

        return NavigationPlan(
            entry_points=plan_data.get("entry_points", []),
            exploration_paths=plan_data.get("exploration_paths", []),
            zoom_instructions=plan_data.get("zoom_instructions", []),
            answer_strategy=plan_data.get("answer_strategy", ""),
            direct_answer=plan_data.get("direct_answer"),
        )
    except json.JSONDecodeError as e:
        # Fallback: create basic plan from graph
        return NavigationPlan(
            entry_points=[],
            exploration_paths=[],
            zoom_instructions=[f"JSON parse failed: {e}. Explore graph manually."],
            answer_strategy="Read top PageRank symbols relevant to task.",
            direct_answer=None,
        )


def build_navigation_prompt(
    plan: NavigationPlan,
    task_description: str,
    workspace_path: Path,
    depth_1_graph: Optional[str] = None,
    depth_3_graph: Optional[str] = None,
) -> str:
    """
    Build a prompt that teaches the agent to navigate the architecture.

    This prompt embeds the hierarchical navigation strategy directly.
    """

    # If graph directly answered the question
    if plan.direct_answer:
        return f"""## ARCHITECTURE GRAPH DIRECT ANSWER

The llmcc architecture graph directly answers your question:

**Answer:** {plan.direct_answer}

If you need to verify or expand on this answer, the relevant symbols are in the graph below.

## TASK
{task_description}

## WORKSPACE
{workspace_path}

The answer above comes from static analysis of the codebase's architecture graph.
You may read specific files if you need implementation details beyond what the graph shows."""

    # Build entry points section
    entry_section = ""
    if plan.entry_points:
        entries = []
        for i, ep in enumerate(plan.entry_points, 1):
            loc = ep.get('file', ep.get('module', ep.get('crate', '?')))
            sym = ep.get('symbol', '?')
            reason = ep.get('reason', '')
            entries.append(f"  {i}. **{sym}** in `{loc}`\n     → {reason}")
        entry_section = f"""
## ENTRY POINTS (Start Here)

{chr(10).join(entries)}
"""

    # Build exploration paths section
    paths_section = ""
    if plan.exploration_paths:
        paths = []
        for p in plan.exploration_paths:
            paths.append(f"  - {p.get('from', '?')} → {p.get('to', '?')} ({p.get('edge_type', 'uses')}): {p.get('reason', '')}")
        paths_section = f"""
## EXPLORATION PATHS (Follow These Edges)

{chr(10).join(paths)}
"""

    # Build zoom instructions section
    zoom_section = ""
    if plan.zoom_instructions:
        zoom_section = f"""
## ZOOM IN/OUT INSTRUCTIONS

{chr(10).join(f'  - {z}' for z in plan.zoom_instructions)}
"""

    # Parse depth 3 graph for quick reference
    symbol_ref = ""
    if depth_3_graph:
        nodes, _, _ = parse_depth_3_graph(depth_3_graph)
        top_10 = []
        for node_id, info in list(nodes.items())[:10]:
            if info.get('path'):
                path = info['path']
                if str(workspace_path) in path:
                    path = path.replace(str(workspace_path) + '/', '')
                top_10.append(f"  - {info['label']} @ {path}")
        if top_10:
            symbol_ref = f"""
## TOP 10 PAGERANK SYMBOLS (Most Central)

{chr(10).join(top_10)}
"""

    return f"""## ARCHITECTURE-GUIDED NAVIGATION

⚠️ **IMPORTANT: DO NOT run llmcc or generate graphs yourself.**
The architecture analysis is already complete and provided below.
Go directly to reading the entry point files.

You have an llmcc architecture graph showing the codebase's structure.
**Use it as a MAP** - don't grep blindly, follow the graph's guidance.

### How to Navigate

1. **START WITH ENTRY POINTS** - Read the files listed in ENTRY POINTS below first.
   These are the highest-relevance locations for your task.

2. **TRUST THE PAGERANK** - Top symbols are most central. If you need "the main X",
   look at the top 20 symbols first.

3. **READ EDGES AS DEPENDENCIES** - `A -> B` means A depends on/uses B.
   - "What uses X?" → find edges pointing TO X
   - "What does X depend on?" → find edges FROM X

4. **ZOOM IN/OUT** - If you need more detail than the graph shows:
   - Missing symbol? It's below PageRank threshold. Read the file directly.
   - Need bigger picture? The graph clusters show module/crate boundaries.
{entry_section}{paths_section}{zoom_section}{symbol_ref}
## ANSWER STRATEGY

{plan.answer_strategy}

## TASK

{task_description}

## WORKSPACE

{workspace_path}

**Begin at the first entry point. Follow the exploration paths. Zoom as needed.**"""


# Legacy compatibility aliases
ReadingPlan = NavigationPlan  # For backward compatibility


def build_plan_prompt(plan: NavigationPlan, task_description: str, workspace_path: Path) -> str:
    """Legacy alias for build_navigation_prompt."""
    return build_navigation_prompt(plan, task_description, workspace_path)


# Synchronous wrapper for use in non-async contexts
def generate_plan_sync(
    task_description: str,
    depth_3_graph: str,
    workspace_path: Path,
    depth_1_graph: Optional[str] = None,
    model: str = "claude-opus-4-5-20251101",
) -> NavigationPlan:
    """Synchronous wrapper for generate_plan."""
    import asyncio
    return asyncio.run(generate_plan(
        task_description,
        depth_3_graph,
        workspace_path,
        depth_1_graph=depth_1_graph,
        model=model,
    ))
