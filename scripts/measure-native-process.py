#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Sample an explicitly selected Linux app process and its descendants.

Does not launch applications, inspect arguments/environment, or claim a budget
passes. CPU percent uses one logical core = 100%; RSS may count shared pages twice.
"""
import argparse
import json
import math
import os
from pathlib import Path
import time


def processes():
    result = {}
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            raw = (entry / 'stat').read_text()
            fields = raw[raw.rfind(')') + 2:].split()
            result[int(entry.name)] = {
                'parent': int(fields[1]), 'start': int(fields[19]),
                'ticks': int(fields[11]) + int(fields[12]),
                'rssBytes': int(fields[21]) * os.sysconf('SC_PAGE_SIZE'),
                'name': raw[raw.find('(') + 1:raw.rfind(')')],
            }
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
    return result


def sample(pid, identity):
    all_processes = processes()
    if pid not in all_processes or all_processes[pid]['start'] != identity:
        raise RuntimeError('Selected app exited or PID was reused')
    selected = {pid}
    while True:
        children = {p for p, info in all_processes.items() if info['parent'] in selected}
        if children <= selected:
            break
        selected |= children
    rows = []
    for p in sorted(selected):
        info = dict(all_processes[p], pid=p)
        try:
            lines = Path(f'/proc/{p}/smaps_rollup').read_text().splitlines()
            info['pssBytes'] = next(int(line.split()[1]) * 1024 for line in lines if line.startswith('Pss:'))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            info['pssBytes'] = None
        rows.append(info)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int)
    parser.add_argument('--settle', type=float, default=30)
    parser.add_argument('--samples', type=int, default=60)
    parser.add_argument('--interval', type=float, default=1)
    args = parser.parse_args()
    if args.samples < 1 or args.interval <= 0 or args.settle < 0:
        parser.error('Use positive sample count/interval and nonnegative settle time')
    identity = processes()[args.pid]['start']
    time.sleep(args.settle)
    previous = {(p['pid'], p['start']): p['ticks'] for p in sample(args.pid, identity)}
    previous_time = time.monotonic()
    started = previous_time
    samples = []
    for _ in range(args.samples):
        time.sleep(args.interval)
        rows = sample(args.pid, identity)
        now = time.monotonic()
        ticks = sum(max(0, p['ticks'] - previous.get((p['pid'], p['start']), p['ticks'])) for p in rows)
        samples.append({
            'elapsedSeconds': now - started, 'rssBytes': sum(p['rssBytes'] for p in rows),
            'pssBytes': sum(p['pssBytes'] for p in rows) if all(p['pssBytes'] is not None for p in rows) else None,
            'cpuPercentOneCore': 100 * ticks / os.sysconf('SC_CLK_TCK') / (now - previous_time),
            'processes': rows,
        })
        previous = {(p['pid'], p['start']): p['ticks'] for p in rows}
        previous_time = now
    def percentile(key, fraction):
        values = sorted(s[key] for s in samples)
        return values[math.ceil(len(values) * fraction) - 1]
    print(json.dumps({
        'pid': args.pid, 'settleSeconds': args.settle, 'samples': samples,
        'medianRssBytes': percentile('rssBytes', .5), 'peakRssBytes': percentile('rssBytes', 1),
        'meanCpuPercentOneCore': sum(s['cpuPercentOneCore'] for s in samples) / len(samples),
        'p95CpuPercentOneCore': percentile('cpuPercentOneCore', .95),
        'comparison': 'not-requested',
        'limitations': 'RSS counts shared pages per process. CPU excludes exited processes between samples and pre-observation work of new children. No cold-start or leak verdict.',
    }, indent=2))


if __name__ == '__main__':
    main()
