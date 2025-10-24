# Default shell for all recipes
set shell := ["/bin/bash", "-c"]

root := justfile_directory()
venv := root + "/venv"

ensure-venv:
    if [ ! -d "{{venv}}" ]; then \
        python3 -m venv "{{venv}}"; \
        . "{{venv}}/bin/activate" && pip install -r "{{root}}/requirements.txt"; \
    fi

build-bindings: ensure-venv
    . "{{venv}}/bin/activate" && maturin develop --manifest-path "{{root}}/crates/llmcc-bindings/Cargo.toml"

run-example: build-bindings
    . "{{venv}}/bin/activate" && python "{{root}}/examples/basic.py"