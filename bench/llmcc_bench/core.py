"""
Core utilities and configuration for llmcc-bench.
Cross-platform support for Windows, Linux, and macOS.
"""

import os
import platform
import re
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # Fallback for older Python

# Package directory
PACKAGE_DIR = Path(__file__).parent.resolve()
PROJECTS_CONFIG = PACKAGE_DIR.parent / "projects.toml"


def find_project_root() -> Path:
    """Find the llmcc project root by looking for Cargo.toml."""
    # Start from package dir and go up
    current = PACKAGE_DIR
    for _ in range(5):  # Max 5 levels up
        if (current / "Cargo.toml").exists():
            return current
        current = current.parent
    # Fallback: assume bench/ is under project root
    return PACKAGE_DIR.parent.parent


# Default paths (can be overridden)
DEFAULT_PROJECT_ROOT = find_project_root()
DEFAULT_SAMPLE_DIR = DEFAULT_PROJECT_ROOT / "sample"


@dataclass
class Project:
    """A sample project for benchmarking."""
    name: str
    github_path: str  # e.g., "BurntSushi/ripgrep"
    language: str = "rust"  # rust, python, typescript, go, etc.

    @property
    def url(self) -> str:
        return f"https://github.com/{self.github_path}.git"


def load_projects(config_path: Optional[Path] = None) -> Dict[str, Project]:
    """Load project definitions from TOML config file."""
    if config_path is None:
        config_path = PROJECTS_CONFIG

    if not config_path.exists():
        # Return empty dict if no config file
        return {}

    try:
        with open(config_path, "rb") as f:
            data = tomllib.load(f)

        projects = {}
        for name, info in data.get("projects", {}).items():
            projects[name] = Project(
                name=name,
                github_path=info.get("github", ""),
                language=info.get("language", "rust"),
            )
        return projects
    except Exception as e:
        print(f"Warning: Failed to load projects config: {e}")
        return {}


# Load projects from config file
PROJECTS: Dict[str, Project] = load_projects()


@dataclass
class SystemInfo:
    """System information for benchmark reports."""
    cpu_model: str
    cpu_physical_cores: int
    cpu_logical_cores: int
    memory_total: str
    memory_available: str
    os_kernel: str
    os_distribution: str
    disk_speed: str


@dataclass
class Config:
    """Configuration for benchmark runs."""
    project_root: Path = field(default_factory=lambda: DEFAULT_PROJECT_ROOT)
    sample_dir: Path = field(default_factory=lambda: DEFAULT_SAMPLE_DIR)
    llmcc_path: Optional[Path] = None
    top_k: int = 200
    depth: int = 3

    def __post_init__(self):
        if self.llmcc_path is None:
            self.llmcc_path = find_llmcc(self.project_root)

    @property
    def benchmark_logs_dir(self) -> Path:
        return self.sample_dir / "benchmark_logs"

    def benchmark_file(self, suffix: str = "", language: str = "") -> Path:
        import platform
        cores = get_cpu_info()[1]  # physical cores
        os_name = platform.system().lower()
        lang_suffix = f"_{language}" if language else ""
        name = f"benchmark_results_{cores}_{os_name}{lang_suffix}{suffix}.md"
        return self.sample_dir / name

    def language_dir(self, language: str) -> Path:
        """Get the output directory for a specific language."""
        return self.sample_dir / language

    @property
    def repos_dir(self) -> Path:
        """Get repos directory (flat under sample)."""
        return self.sample_dir / "repos"

    def project_repo_path(self, project: Project) -> Path:
        """Get source repo path for a project."""
        return self.repos_dir / project.name

    def project_output_dir(self, project: Project, suffix: str = "") -> Path:
        """Get output directory for a project (organized by language)."""
        name = f"{project.name}{suffix}" if suffix else project.name
        return self.language_dir(project.language) / name


def find_llmcc(project_root: Optional[Path] = None) -> Optional[Path]:
    """Find the llmcc binary, checking common locations."""
    if project_root is None:
        project_root = DEFAULT_PROJECT_ROOT

    # Check environment variable first
    if llmcc_env := os.environ.get("LLMCC"):
        llmcc_path = Path(llmcc_env)
        if llmcc_path.exists():
            return llmcc_path

    # Platform-specific binary name
    binary_name = "llmcc.exe" if platform.system() == "Windows" else "llmcc"

    # Check common target paths
    candidates = [
        project_root / "target" / "release" / binary_name,
        project_root / "target" / "x86_64-unknown-linux-gnu" / "release" / binary_name,
        project_root / "target" / "x86_64-pc-windows-msvc" / "release" / binary_name,
        project_root / "target" / "aarch64-apple-darwin" / "release" / binary_name,
        project_root / "target" / "x86_64-apple-darwin" / "release" / binary_name,
    ]

    for candidate in candidates:
        if candidate.exists():
            return candidate

    return None


def get_cpu_info() -> Tuple[str, int, int]:
    """
    Get CPU information.
    Returns: (model_name, physical_cores, logical_cores)
    """
    system = platform.system()
    model = "Unknown"
    physical = os.cpu_count() or 1
    logical = os.cpu_count() or 1

    try:
        if system == "Windows":
            import winreg
            key = winreg.OpenKey(
                winreg.HKEY_LOCAL_MACHINE,
                r"HARDWARE\DESCRIPTION\System\CentralProcessor\0"
            )
            model = winreg.QueryValueEx(key, "ProcessorNameString")[0].strip()
            winreg.CloseKey(key)
            # Get physical cores via WMIC
            result = subprocess.run(
                ["wmic", "cpu", "get", "NumberOfCores"],
                capture_output=True, text=True
            )
            for line in result.stdout.strip().split('\n'):
                if line.strip().isdigit():
                    physical = int(line.strip())
                    break
        elif system == "Linux":
            with open("/proc/cpuinfo") as f:
                for line in f:
                    if "model name" in line:
                        model = line.split(":")[1].strip()
                        break
            # Get physical cores
            result = subprocess.run(
                ["lscpu"], capture_output=True, text=True
            )
            cores_per_socket = 1
            sockets = 1
            for line in result.stdout.split('\n'):
                if "Core(s) per socket:" in line:
                    cores_per_socket = int(line.split(":")[1].strip())
                elif "Socket(s):" in line:
                    sockets = int(line.split(":")[1].strip())
            physical = cores_per_socket * sockets
        elif system == "Darwin":  # macOS
            result = subprocess.run(
                ["sysctl", "-n", "machdep.cpu.brand_string"],
                capture_output=True, text=True
            )
            model = result.stdout.strip()
            result = subprocess.run(
                ["sysctl", "-n", "hw.physicalcpu"],
                capture_output=True, text=True
            )
            physical = int(result.stdout.strip())
            result = subprocess.run(
                ["sysctl", "-n", "hw.logicalcpu"],
                capture_output=True, text=True
            )
            logical = int(result.stdout.strip())
    except Exception:
        pass

    return model, physical, logical


def get_memory_info() -> Tuple[str, str]:
    """
    Get memory information.
    Returns: (total_memory, available_memory) as human-readable strings
    """
    system = platform.system()
    total = "Unknown"
    available = "Unknown"

    def format_bytes(b: int) -> str:
        """Format bytes as human-readable string."""
        for unit in ['B', 'K', 'M', 'G', 'T']:
            if b < 1024:
                return f"{b:.1f}{unit}"
            b /= 1024
        return f"{b:.1f}P"

    try:
        if system == "Windows":
            result = subprocess.run(
                ["wmic", "OS", "get", "TotalVisibleMemorySize,FreePhysicalMemory", "/format:csv"],
                capture_output=True, text=True
            )
            lines = [l for l in result.stdout.strip().split('\n') if l.strip() and not l.startswith("Node")]
            if lines:
                parts = lines[0].split(',')
                if len(parts) >= 3:
                    free_kb = int(parts[1])
                    total_kb = int(parts[2])
                    total = format_bytes(total_kb * 1024)
                    available = format_bytes(free_kb * 1024)
        elif system == "Linux":
            with open("/proc/meminfo") as f:
                meminfo = {}
                for line in f:
                    parts = line.split(":")
                    if len(parts) == 2:
                        key = parts[0].strip()
                        value = parts[1].strip().split()[0]
                        meminfo[key] = int(value) * 1024  # Convert from kB
                total = format_bytes(meminfo.get("MemTotal", 0))
                available = format_bytes(meminfo.get("MemAvailable", 0))
        elif system == "Darwin":
            result = subprocess.run(
                ["sysctl", "-n", "hw.memsize"],
                capture_output=True, text=True
            )
            total_bytes = int(result.stdout.strip())
            total = format_bytes(total_bytes)
            result = subprocess.run(
                ["vm_stat"],
                capture_output=True, text=True
            )
            for line in result.stdout.split('\n'):
                if "Pages free:" in line:
                    free_pages = int(line.split(":")[1].strip().rstrip('.'))
                    available = format_bytes(free_pages * 4096)
                    break
    except Exception:
        pass

    return total, available


def get_os_info() -> Tuple[str, str]:
    """
    Get OS information.
    Returns: (kernel_version, distribution)
    """
    system = platform.system()
    kernel = platform.release()
    distribution = platform.platform()

    try:
        if system == "Linux" and os.path.exists("/etc/os-release"):
            with open("/etc/os-release") as f:
                for line in f:
                    if line.startswith("PRETTY_NAME="):
                        distribution = line.split("=")[1].strip().strip('"')
                        break
        elif system == "Darwin":
            result = subprocess.run(
                ["sw_vers", "-productName"],
                capture_output=True, text=True
            )
            name = result.stdout.strip()
            result = subprocess.run(
                ["sw_vers", "-productVersion"],
                capture_output=True, text=True
            )
            version = result.stdout.strip()
            distribution = f"{name} {version}"
        elif system == "Windows":
            distribution = f"Microsoft Windows {platform.win32_ver()[0]} {platform.win32_edition()}"
            kernel = f"Windows {platform.version()}"
    except Exception:
        pass

    return kernel, distribution


def get_disk_speed() -> str:
    """
    Measure disk write speed.
    Returns human-readable speed string (e.g., '1.2 GB/s').
    Cross-platform support for Windows, Linux, and macOS.
    """
    import tempfile
    import time

    system = platform.system()
    test_size_mb = 256
    test_size_bytes = test_size_mb * 1024 * 1024

    def format_speed(bytes_per_sec: float) -> str:
        """Format speed as human-readable string."""
        if bytes_per_sec >= 1e9:
            return f"{bytes_per_sec / 1e9:.1f} GB/s"
        elif bytes_per_sec >= 1e6:
            return f"{bytes_per_sec / 1e6:.1f} MB/s"
        else:
            return f"{bytes_per_sec / 1e3:.1f} KB/s"

    try:
        # Create a temporary file for testing
        with tempfile.NamedTemporaryFile(delete=False) as tmp:
            tmp_path = tmp.name

        if system == "Windows":
            # Measure write speed
            data = os.urandom(test_size_bytes)
            start = time.perf_counter()
            with open(tmp_path, 'wb') as f:
                f.write(data)
                f.flush()
                os.fsync(f.fileno())
            elapsed = time.perf_counter() - start

            os.unlink(tmp_path)
            return format_speed(test_size_bytes / elapsed)

        else:  # Linux and macOS
            # Measure write speed using dd with fdatasync (ensures data hits disk)
            write_result = subprocess.run(
                [
                    "dd",
                    "if=/dev/zero",
                    f"of={tmp_path}",
                    "bs=1M",
                    f"count={test_size_mb}",
                    "conv=fdatasync",
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )

            os.unlink(tmp_path)

            # Parse the speed from dd output (in stderr)
            # Format: "268435456 bytes (268 MB, 256 MiB) copied, 0.123 s, 2.2 GB/s"
            # Note: dd output may contain newlines, so normalize whitespace first
            output = ' '.join(write_result.stderr.split())
            if ", " in output:
                parts = output.split(", ")
                for part in parts:
                    if "/s" in part:
                        return part.strip()

            return "unknown"

    except Exception:
        # Clean up temp file if it exists
        try:
            os.unlink(tmp_path)
        except Exception:
            pass
        return "unknown"


def get_system_info() -> SystemInfo:
    """Get complete system information."""
    cpu_model, cpu_physical, cpu_logical = get_cpu_info()
    mem_total, mem_available = get_memory_info()
    os_kernel, os_distro = get_os_info()
    disk_speed = get_disk_speed()

    return SystemInfo(
        cpu_model=cpu_model,
        cpu_physical_cores=cpu_physical,
        cpu_logical_cores=cpu_logical,
        memory_total=mem_total,
        memory_available=mem_available,
        os_kernel=os_kernel,
        os_distribution=os_distro,
        disk_speed=disk_speed,
    )


def count_rust_files(src_dir: Path) -> int:
    """Count number of .rs files in a directory."""
    return count_files(src_dir, "rust")


def count_files(src_dir: Path, language: str = "rust") -> int:
    """Count number of source files in a directory for a given language.

    Excludes common non-source directories like node_modules, tests, etc.
    """
    if not src_dir.exists():
        return 0

    extensions = {
        "rust": [".rs"],
        "typescript": [".ts", ".tsx"],
        "python": [".py"],
    }

    # Directories to skip (common non-source directories)
    skip_dirs = {
        "node_modules", "tests", "test", "__tests__",
        "baselines", "fixtures", "examples", "dist", "build",
        ".git", "target", "__pycache__", ".tox", "venv",
    }

    exts = extensions.get(language, [".rs"])
    count = 0
    for root, dirs, files in os.walk(src_dir):
        # Modify dirs in-place to skip unwanted directories
        dirs[:] = [d for d in dirs if d not in skip_dirs]
        for f in files:
            if any(f.endswith(ext) for ext in exts):
                count += 1
    return count


def count_loc(src_dir: Path, use_estimate: bool = False) -> int:
    """
    Count lines of code in Rust files.
    Excludes blank lines and comment-only lines.

    Args:
        src_dir: Directory to count
        use_estimate: If True, use file count * 200 as quick estimate
    """
    if not src_dir.exists():
        return 0

    # Quick estimate mode
    if use_estimate:
        file_count = count_rust_files(src_dir)
        return file_count * 200  # ~200 lines per file average

    # Install tokei if not available (cargo must exist for Rust projects)
    if not shutil.which("tokei"):
        try:
            print("Installing tokei for accurate LoC counting...")
            subprocess.run(
                ["cargo", "install", "tokei"],
                capture_output=True, text=True, timeout=300
            )
        except Exception:
            pass

    # Try tokei (most accurate and fast)
    if shutil.which("tokei"):
        try:
            result = subprocess.run(
                ["tokei", str(src_dir), "-t", "Rust", "-o", "json"],
                capture_output=True, text=True, timeout=30
            )
            import json
            data = json.loads(result.stdout)
            if "Rust" in data:
                return data["Rust"].get("code", 0)
        except Exception:
            pass

    # Fallback: count lines manually (excludes comments and blank lines)
    total_lines = 0
    in_block_comment = False
    for root, _, files in os.walk(src_dir):
        for f in files:
            if f.endswith('.rs'):
                try:
                    filepath = Path(root) / f
                    with open(filepath, 'r', encoding='utf-8', errors='ignore') as fp:
                        for line in fp:
                            stripped = line.strip()

                            # Handle block comments
                            if in_block_comment:
                                if '*/' in stripped:
                                    in_block_comment = False
                                    # Check if there's code after the block comment ends
                                    after_comment = stripped[stripped.index('*/') + 2:].strip()
                                    if after_comment and not after_comment.startswith('//'):
                                        total_lines += 1
                                continue

                            if not stripped:
                                continue

                            # Skip single-line comments
                            if stripped.startswith('//'):
                                continue

                            # Check for block comment start
                            if '/*' in stripped:
                                # Check if it's a single-line block comment
                                if '*/' in stripped[stripped.index('/*') + 2:]:
                                    # Remove the block comment and check remaining
                                    before = stripped[:stripped.index('/*')].strip()
                                    after_end = stripped[stripped.index('*/') + 2:].strip()
                                    if before or (after_end and not after_end.startswith('//')):
                                        total_lines += 1
                                else:
                                    in_block_comment = True
                                    # Count if there's code before the comment
                                    before = stripped[:stripped.index('/*')].strip()
                                    if before:
                                        total_lines += 1
                                continue

                            total_lines += 1
                except Exception:
                    pass
    return total_lines


def count_graph_stats(dot_file: Path) -> Tuple[int, int]:
    """
    Count nodes and edges in a DOT file.
    Returns: (node_count, edge_count)
    """
    if not dot_file.exists():
        return 0, 0

    nodes = 0
    edges = 0
    node_pattern = re.compile(r'^\s+n\d+\[label=')

    try:
        with open(dot_file, 'r', encoding='utf-8', errors='ignore') as f:
            for line in f:
                if node_pattern.match(line):
                    nodes += 1
                if '->' in line:
                    edges += 1
    except Exception:
        pass
    return nodes, edges


def format_loc(loc: int) -> str:
    """Format lines of code as human-readable (e.g., ~92K)."""
    if loc >= 1000:
        return f"~{(loc + 500) // 1000}K"
    return str(loc)


def format_time(seconds: float) -> str:
    """Format time in seconds with appropriate precision."""
    if seconds < 0.01:
        return "0.00s"
    elif seconds < 10:
        return f"{seconds:.2f}s"
    else:
        return f"{seconds:.1f}s"


def run_command(
    cmd: List[str],
    env: Optional[Dict[str, str]] = None,
    capture: bool = True,
    timeout: Optional[int] = None,
    cwd: Optional[Path] = None,
) -> subprocess.CompletedProcess:
    """Run a command with cross-platform support."""
    full_env = os.environ.copy()
    if env:
        full_env.update(env)

    return subprocess.run(
        cmd,
        env=full_env,
        capture_output=capture,
        text=True,
        timeout=timeout,
        cwd=cwd,
    )
