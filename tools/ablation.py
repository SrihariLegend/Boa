#!/usr/bin/env python3
"""
Ablation testing for Karpov chess engine eval terms.

For each eval term, we:
1. Comment it out (set its contribution to zero)
2. Rebuild the engine
3. Run bench at a fixed depth
4. Compare total nodes and best moves vs baseline

A term that HELPS the engine will cause MORE nodes (worse pruning) or different
best moves when removed. A term that HURTS will cause FEWER nodes (better pruning)
when removed.

The key metric is: does removing this term change search behavior? If nodes go DOWN,
the term was likely adding noise. If nodes go UP, the term was helping the search
make better pruning decisions.
"""

import subprocess
import os
import re
import sys
import shutil
import time

ENGINE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EVAL_RS = os.path.join(ENGINE_DIR, "src", "eval.rs")
ENGINE_BIN = os.path.join(ENGINE_DIR, "target", "release", "karpov")
BENCH_DEPTH = 8

# Eval terms to test: (name, function_name, zero_return_value)
# Each function returns either (i32, i32) for mg/eg or i32 for single-phase
EVAL_TERMS = [
    ("freedom_metric",        "freedom_metric",         "0"),
    ("prophylaxis_eval",      "prophylaxis_eval",       "(0, 0)"),
    ("piece_coordination",    "piece_coordination_eval","(0, 0)"),
    ("weak_square_eval",      "weak_square_eval",       "(0, 0)"),
    ("knight_vs_bishop",      "knight_vs_bishop_eval",  "(0, 0)"),
    ("bad_bishop_eval",       "bad_bishop_eval",        "(0, 0)"),
    ("space_evaluation",      "space_evaluation",       "(0, 0)"),
    ("trade_down_bonus",      "trade_down_bonus",       "(0, 0)"),
    ("king_safety",           "king_safety",            "(0, 0)"),
    ("mobility_and_activity", "mobility_and_activity",  "(0, 0)"),
    ("pawn_structure",        "pawn_structure",         "(0, 0)"),
]


def run_command(cmd, timeout=60):
    """Run a command and return stdout."""
    try:
        result = subprocess.run(
            cmd, shell=True, capture_output=True, text=True, timeout=timeout,
            cwd=ENGINE_DIR
        )
        return result.stdout + result.stderr
    except subprocess.TimeoutExpired:
        return "TIMEOUT"


def build_engine():
    """Build the engine in release mode. Returns True on success."""
    out = run_command("cargo build --release -j 24 2>&1", timeout=30)
    return "Finished" in out


def run_bench():
    """Run bench and return (total_nodes, per_position_results)."""
    cmd = f'printf "bench {BENCH_DEPTH}\\nquit\\n" | timeout 30 {ENGINE_BIN}'
    out = run_command(cmd, timeout=45)
    
    # Parse bench lines
    positions = []
    total_nodes = 0
    for line in out.split('\n'):
        m = re.match(r'bench (\d+)/\d+: (\d+) nodes\s+score (-?\d+) pv (\S+)', line)
        if m:
            positions.append({
                'idx': int(m.group(1)),
                'nodes': int(m.group(2)),
                'score': int(m.group(3)),
                'pv': m.group(4),
            })
        m2 = re.match(r'Total nodes\s*:\s*(\d+)', line)
        if m2:
            total_nodes = int(m2.group(1))
    
    if total_nodes == 0 and positions:
        total_nodes = sum(p['nodes'] for p in positions)
    
    return total_nodes, positions


def disable_eval_term(original_code, func_name, zero_value):
    """
    Add an early return at the top of the function body to disable it.
    Finds 'fn func_name(' and inserts 'return zero_value;' after the opening brace.
    """
    # Find the function definition
    pattern = rf'(fn {func_name}\([^)]*\)[^{{]*\{{)'
    match = re.search(pattern, original_code)
    if not match:
        print(f"  WARNING: Could not find function {func_name}")
        return None
    
    insert_pos = match.end()
    modified = original_code[:insert_pos] + f'\n    return {zero_value}; // ABLATION: disabled\n' + original_code[insert_pos:]
    return modified


def main():
    print(f"Karpov Engine Ablation Testing")
    print(f"Bench depth: {BENCH_DEPTH}")
    print(f"=" * 60)
    
    # Read original source
    with open(EVAL_RS, 'r') as f:
        original_code = f.read()
    
    # Backup
    backup_path = EVAL_RS + ".bak"
    shutil.copy2(EVAL_RS, backup_path)
    
    try:
        # Run baseline
        print("\n[BASELINE] Building and running bench...")
        if not build_engine():
            print("ERROR: Baseline build failed!")
            return
        
        baseline_nodes, baseline_positions = run_bench()
        if baseline_nodes == 0:
            print("ERROR: Baseline bench returned 0 nodes!")
            return
        
        print(f"  Baseline: {baseline_nodes:,} nodes")
        print(f"  Positions: {len(baseline_positions)}")
        baseline_pvs = {p['idx']: p['pv'] for p in baseline_positions}
        baseline_scores = {p['idx']: p['score'] for p in baseline_positions}
        
        # Test each term
        results = []
        for name, func_name, zero_val in EVAL_TERMS:
            print(f"\n[{name}] Disabling...")
            
            modified = disable_eval_term(original_code, func_name, zero_val)
            if modified is None:
                results.append((name, 0, 0, 0, "SKIP"))
                continue
            
            with open(EVAL_RS, 'w') as f:
                f.write(modified)
            
            if not build_engine():
                print(f"  ERROR: Build failed for {name}")
                results.append((name, 0, 0, 0, "BUILD_FAIL"))
                continue
            
            nodes, positions = run_bench()
            if nodes == 0:
                print(f"  ERROR: Bench returned 0 nodes for {name}")
                results.append((name, 0, 0, 0, "BENCH_FAIL"))
                continue
            
            # Compare
            node_diff = nodes - baseline_nodes
            node_pct = (node_diff / baseline_nodes) * 100
            
            # Count PV changes
            pv_changes = 0
            score_diff_sum = 0
            for p in positions:
                if p['idx'] in baseline_pvs and p['pv'] != baseline_pvs[p['idx']]:
                    pv_changes += 1
                if p['idx'] in baseline_scores:
                    score_diff_sum += abs(p['score'] - baseline_scores[p['idx']])
            
            print(f"  Nodes: {nodes:,} ({node_pct:+.1f}%)")
            print(f"  PV changes: {pv_changes}/{len(positions)}")
            print(f"  Avg score diff: {score_diff_sum/max(len(positions),1):.1f} cp")
            
            results.append((name, nodes, node_pct, pv_changes, score_diff_sum / max(len(positions), 1)))
        
        # Restore original
        shutil.copy2(backup_path, EVAL_RS)
        build_engine()
        
        # Summary
        print("\n" + "=" * 60)
        print("ABLATION RESULTS SUMMARY")
        print("=" * 60)
        print(f"{'Term':<25} {'Nodes':>12} {'Δ Nodes%':>10} {'PV Δ':>6} {'Avg Score Δ':>12}")
        print("-" * 65)
        print(f"{'BASELINE':<25} {baseline_nodes:>12,} {'0.0%':>10} {'-':>6} {'-':>12}")
        
        for name, nodes, pct, pv_ch, score_d in results:
            if isinstance(score_d, str):
                print(f"{name:<25} {'---':>12} {'---':>10} {'---':>6} {score_d:>12}")
            else:
                effect = "NOISE" if pct < -2 else "HELPS" if pct > 2 else "NEUTRAL"
                if pv_ch >= 10:
                    effect += "*"
                print(f"{name:<25} {nodes:>12,} {pct:>+9.1f}% {pv_ch:>6} {score_d:>11.1f} {effect}")
        
        print()
        print("Legend:")
        print("  HELPS  = removing it increased nodes (term was helping pruning)")
        print("  NOISE  = removing it decreased nodes (term was adding noise)")
        print("  NEUTRAL = minimal change (<2%)")
        print("  * = many PV changes (term significantly affects move choice)")
        
    finally:
        # Always restore original
        if os.path.exists(backup_path):
            shutil.copy2(backup_path, EVAL_RS)
            os.remove(backup_path)
            build_engine()


if __name__ == "__main__":
    main()
