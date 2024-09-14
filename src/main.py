import argparse

from src.parser import parse_from_file
from src.compiler import compile_graph
from src.printer import write_graph, print_graph
from src.config import *

def main():
    parser = argparse.ArgumentParser(description="ai compiler")
    parser.add_argument("-c", "--source", metavar="SOURCE", help="Source file to compile")
    parser.add_argument("-I", "--include-dir", metavar="DIR", help="Include directory")
    parser.add_argument("-o", "--output", metavar="OUTPUT", help="Output file")

    args = parser.parse_args()

    log.info(f"ai compiling {args.source}")
    g = parse_from_file(args.source)
    compile_graph(g)
    write_graph(g, args.output)


if __name__ == "__main__":
    main()