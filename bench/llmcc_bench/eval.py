"""
Evaluation module for benchmark results.

Compares baseline vs llmcc answers head-to-head using LLM-as-judge.
"""

import asyncio
import json
import os
import urllib.request
import urllib.error
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .agent.metrics import TaskMetrics, load_metrics
from .agent.tasks import Task, get_task_by_id


COMPARISON_PROMPT = """You are an expert code reviewer comparing two AI agents' answers to a codebase exploration question.

## Task Question
{question}

## Answer A (Baseline - no graph context)
{answer_a}

## Answer B (With llmcc graph context)
{answer_b}

## Comparison Criteria

Compare the two answers on these dimensions:

1. **Completeness**: Which answer addresses more parts of the question?
2. **Accuracy**: Which answer has fewer errors or hallucinations?
3. **Specificity**: Which answer provides more precise file paths, function names, and code references?
4. **Understanding**: Which answer demonstrates better understanding of how components relate?

## Response Format

Respond with ONLY a JSON object (no markdown, no explanation outside the JSON):
{{
  "winner": "<A|B|tie>",
  "winner_score": <1-10>,
  "loser_score": <1-10>,
  "margin": "<decisive|moderate|slight|tie>",
  "reasoning": "<1-2 sentence explanation of why one is better>"
}}

Guidelines:
- "winner": Which answer is better overall ("A", "B", or "tie")
- "winner_score": Score for the better answer (1-10 scale)
- "loser_score": Score for the worse answer (1-10 scale)
- "margin": How much better is the winner?
  - "decisive": Clear winner, significantly better
  - "moderate": Noticeably better but not overwhelming
  - "slight": Marginally better
  - "tie": Effectively equal quality
- Scoring: 1-2 poor, 3-4 below average, 5-6 acceptable, 7-8 good, 9-10 excellent
"""


@dataclass
class ComparisonResult:
    """Result of comparing baseline vs llmcc answers."""

    task_id: str
    run_id: str

    winner: str = "tie"  # "baseline", "llmcc", or "tie"
    baseline_score: int = 0
    llmcc_score: int = 0
    margin: str = "tie"  # "decisive", "moderate", "slight", "tie"
    reasoning: str = ""

    error: Optional[str] = None

    @property
    def is_valid(self) -> bool:
        return self.error is None and self.baseline_score > 0

    @property
    def llmcc_delta(self) -> int:
        """Score improvement for llmcc (positive = llmcc better)."""
        return self.llmcc_score - self.baseline_score

    def to_dict(self) -> dict:
        return {
            "task_id": self.task_id,
            "run_id": self.run_id,
            "winner": self.winner,
            "baseline_score": self.baseline_score,
            "llmcc_score": self.llmcc_score,
            "margin": self.margin,
            "reasoning": self.reasoning,
            "error": self.error,
        }

    @classmethod
    def from_dict(cls, data: dict) -> "ComparisonResult":
        return cls(
            task_id=data["task_id"],
            run_id=data["run_id"],
            winner=data.get("winner", "tie"),
            baseline_score=data.get("baseline_score", 0),
            llmcc_score=data.get("llmcc_score", 0),
            margin=data.get("margin", "tie"),
            reasoning=data.get("reasoning", ""),
            error=data.get("error"),
        )


class Evaluator:
    """Compares baseline vs llmcc answers using LLM-as-judge via the bridge."""

    def __init__(
        self,
        model: str = "claude-opus-4-20250514",
        base_url: str = "http://localhost:5168",
    ):
        self.model = model
        self.base_url = base_url

    async def compare_answers(
        self,
        question: str,
        baseline_answer: str,
        llmcc_answer: str,
        task_id: str,
        run_id: str = "0",
        timeout: float = 60.0,
    ) -> ComparisonResult:
        """Compare baseline vs llmcc answers head-to-head."""

        # Handle missing answers
        if not baseline_answer or not baseline_answer.strip():
            if not llmcc_answer or not llmcc_answer.strip():
                return ComparisonResult(
                    task_id=task_id, run_id=run_id,
                    error="Both answers are empty"
                )
            # llmcc answered, baseline didn't
            return ComparisonResult(
                task_id=task_id, run_id=run_id,
                winner="llmcc", baseline_score=1, llmcc_score=5,
                margin="decisive", reasoning="Baseline produced no answer"
            )

        if not llmcc_answer or not llmcc_answer.strip():
            # baseline answered, llmcc didn't
            return ComparisonResult(
                task_id=task_id, run_id=run_id,
                winner="baseline", baseline_score=5, llmcc_score=1,
                margin="decisive", reasoning="llmcc produced no answer"
            )

        prompt = COMPARISON_PROMPT.format(
            question=question.strip(),
            answer_a=baseline_answer.strip(),
            answer_b=llmcc_answer.strip(),
        )

        try:
            # Use the bridge's OpenAI-compatible endpoint
            url = f"{self.base_url}/v1/chat/completions"
            data = json.dumps({
                "model": self.model,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.0,
                "max_tokens": 500,
            }).encode("utf-8")

            req = urllib.request.Request(
                url,
                data=data,
                headers={
                    "Content-Type": "application/json",
                    "Authorization": "Bearer sk-copilot-bridge",
                },
            )

            # Run in thread pool to not block
            loop = asyncio.get_event_loop()
            response_data = await loop.run_in_executor(
                None,
                lambda: urllib.request.urlopen(req, timeout=timeout).read()
            )

            resp = json.loads(response_data)
            content = resp["choices"][0]["message"]["content"].strip()

            # Handle markdown code blocks
            if content.startswith("```"):
                content = content.split("```")[1]
                if content.startswith("json"):
                    content = content[4:]
                content = content.strip()

            result = json.loads(content)

            # Map winner from A/B to baseline/llmcc
            raw_winner = result.get("winner", "tie").upper()
            if raw_winner == "A":
                winner = "baseline"
                baseline_score = int(result.get("winner_score", 3))
                llmcc_score = int(result.get("loser_score", 2))
            elif raw_winner == "B":
                winner = "llmcc"
                llmcc_score = int(result.get("winner_score", 3))
                baseline_score = int(result.get("loser_score", 2))
            else:
                winner = "tie"
                # For ties, use winner_score for both or average
                score = int(result.get("winner_score", 3))
                baseline_score = score
                llmcc_score = score

            return ComparisonResult(
                task_id=task_id,
                run_id=run_id,
                winner=winner,
                baseline_score=baseline_score,
                llmcc_score=llmcc_score,
                margin=result.get("margin", "tie"),
                reasoning=result.get("reasoning", ""),
            )

        except json.JSONDecodeError as e:
            return ComparisonResult(
                task_id=task_id, run_id=run_id,
                error=f"Failed to parse response: {e}"
            )
        except urllib.error.URLError as e:
            return ComparisonResult(
                task_id=task_id, run_id=run_id,
                error=f"Bridge connection failed: {e}. Is the bridge running?"
            )
        except Exception as e:
            return ComparisonResult(
                task_id=task_id, run_id=run_id,
                error=f"Evaluation failed: {e}"
            )
