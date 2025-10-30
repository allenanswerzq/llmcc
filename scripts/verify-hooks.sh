#!/bin/bash
# Verification script for git hooks
# Run this to verify all hooks are properly installed and working

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Git Hooks Verification Report       ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"

ISSUES=0

# Check 1: pre-commit hook exists
echo -e "\n${YELLOW}Checking pre-commit hook...${NC}"
if [ -f ".git/hooks/pre-commit" ] && [ -x ".git/hooks/pre-commit" ]; then
    echo -e "${GREEN}✓ pre-commit hook installed and executable${NC}"
else
    echo -e "${RED}✗ pre-commit hook missing or not executable${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 2: pre-push hook exists
echo -e "\n${YELLOW}Checking pre-push hook...${NC}"
if [ -f ".git/hooks/pre-push" ] && [ -x ".git/hooks/pre-push" ]; then
    echo -e "${GREEN}✓ pre-push hook installed and executable${NC}"
else
    echo -e "${RED}✗ pre-push hook missing or not executable${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 3: Templates exist
echo -e "\n${YELLOW}Checking hook templates...${NC}"
if [ -f "scripts/git-hooks/pre-commit" ] && [ -f "scripts/git-hooks/pre-push" ]; then
    echo -e "${GREEN}✓ Hook templates present in scripts/git-hooks/${NC}"
else
    echo -e "${RED}✗ Hook templates missing${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 4: Setup script exists
echo -e "\n${YELLOW}Checking setup script...${NC}"
if [ -f "scripts/setup-hooks.sh" ] && [ -x "scripts/setup-hooks.sh" ]; then
    echo -e "${GREEN}✓ Setup script present and executable${NC}"
else
    echo -e "${RED}✗ Setup script missing or not executable${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 5: Documentation exists
echo -e "\n${YELLOW}Checking documentation...${NC}"
if [ -f "scripts/GIT_HOOKS_README.md" ] && [ -f "GIT_HOOKS_SETUP.md" ]; then
    echo -e "${GREEN}✓ Documentation present${NC}"
else
    echo -e "${RED}✗ Documentation missing${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 6: Cargo.toml exists
echo -e "\n${YELLOW}Checking project configuration...${NC}"
if [ -f "Cargo.toml" ]; then
    echo -e "${GREEN}✓ Cargo.toml found${NC}"
else
    echo -e "${RED}✗ Cargo.toml missing${NC}"
    ISSUES=$((ISSUES + 1))
fi

# Check 7: Required tools
echo -e "\n${YELLOW}Checking required tools...${NC}"
TOOLS_OK=true

if command -v cargo &> /dev/null; then
    echo -e "${GREEN}✓ cargo installed${NC}"
else
    echo -e "${RED}✗ cargo not found${NC}"
    TOOLS_OK=false
fi

if command -v rustfmt &> /dev/null; then
    echo -e "${GREEN}✓ rustfmt installed${NC}"
else
    echo -e "${YELLOW}⚠ rustfmt not found (needed for formatting)${NC}"
    TOOLS_OK=false
fi

if cargo clippy --version &> /dev/null; then
    echo -e "${GREEN}✓ clippy installed${NC}"
else
    echo -e "${YELLOW}⚠ clippy not found (will be downloaded on first use)${NC}"
fi

# Summary
echo -e "\n${BLUE}════════════════════════════════════════${NC}"
if [ $ISSUES -eq 0 ] && [ "$TOOLS_OK" = true ]; then
    echo -e "${GREEN}All checks passed! Git hooks are ready to use.${NC}"
    exit 0
elif [ $ISSUES -eq 0 ]; then
    echo -e "${YELLOW}Hooks installed, but some tools need setup.${NC}"
    echo -e "${YELLOW}Run: rustup component add rustfmt${NC}"
    exit 0
else
    echo -e "${RED}$ISSUES issue(s) found. Please run: ./scripts/setup-hooks.sh${NC}"
    exit 1
fi
