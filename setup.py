#!/usr/bin/env python3
"""
Setup script for llmcc Python package.
This allows installation via pip with automatic Rust compilation.
"""

from setuptools import setup, find_packages

setup(
    name="llmcc",
    version="0.2.50",
    description="LLM Context Compiler - Universal context builder for any language and document type",
    long_description=open("README.md").read() if __import__("os").path.exists("README.md") else "",
    long_description_content_type="text/markdown",
    author="llmcc contributors",
    url="https://github.com/allenanswerzq/llmcc",
    license="Apache-2.0",
    packages=find_packages(exclude=["tests", "examples"]),
    python_requires=">=3.8",
    zip_safe=False,
    extras_require={
        "dev": [
            "pytest>=7.0",
            "pytest-cov>=4.0",
            "black>=23.0",
            "ruff>=0.1.0",
            "mypy>=1.0",
        ],
    },
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: Apache Software License",
        "Operating System :: OS Independent",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
        "Programming Language :: Rust",
        "Topic :: Software Development :: Libraries :: Python Modules",
    ],
)
