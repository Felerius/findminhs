#!/usr/bin/env python3
import dataclasses
import json
import re
from pathlib import Path
from typing import List

from lib import run

SCRIPT_DIR = Path(__file__).resolve().parent
INSTANCE_DIR = SCRIPT_DIR / 'instances'
RESULTS_DIR = SCRIPT_DIR / 'results'
LOGS_DIR = SCRIPT_DIR / 'logs'
SETTINGS_DIR = SCRIPT_DIR / 'settings'

TIMEOUT = '1h'


@dataclasses.dataclass
class Settings:
    enable_local_search: bool = False
    enable_max_degree_bound: bool = True
    enable_sum_degree_bound: bool = False
    enable_efficiency_bound: bool = True
    enable_packing_bound: bool = True
    enable_sum_over_packing_bound: bool = True
    packing_from_scratch_limit: int = 3
    greedy_mode: str = 'Once'


def find_instances() -> List[str]:
    instances = []
    for f in INSTANCE_DIR.glob('**/*.dat'):
        f = f.relative_to(INSTANCE_DIR)
        if f.parts[0] != 'outdated':
            instances.append(f.with_suffix(''))

    # Use file size as an estimation for solving difficulty. Solving easy
    # instances early allows us to look at partial results early on.
    instances.sort(key=lambda f: (INSTANCE_DIR / f.parent /
                                  (f.name + '.dat')).stat().st_size)

    return instances


def add_findminhs_experiment(name: str, settings: Settings,
                             instances: List[str]) -> None:
    results_dir = RESULTS_DIR / name
    logs_dir = LOGS_DIR / name
    settings_file = SETTINGS_DIR / (name + '.json')
    for directory in (results_dir, logs_dir, SETTINGS_DIR):
        directory.mkdir(exist_ok=True, parents=True)
    with settings_file.open('w') as f:
        json.dump(dataclasses.asdict(settings), f)
    command = (f'timeout {TIMEOUT} {SCRIPT_DIR}/findminhs solve '
               f'{INSTANCE_DIR}/[[name]].dat '
               f'{settings_file} '
               f'--report {results_dir}/[[name]].json 2>&1')
    run.add(
        name,
        command,
        {'name': instances},
        stdout_file=f'{logs_dir}/[[name]].log',
        creates_file=f'{results_dir}/[[name]].json',
        allowed_return_codes=[0, 124],
    )


def main() -> None:
    instances = [f.stem for f in INSTANCE_DIR.glob('*.dat')]
    run.group('all')

    # All default settings
    add_findminhs_experiment('default', Settings(), instances)

    # Different packing from scratch limits
    for i in [0, 1, 5, 7, 9, 11, 13, 15, 17, 19]:
        add_findminhs_experiment(f'from-scratch-{i}',
                                 Settings(packing_from_scratch_limit=i),
                                 instances)

    # Different experiments using only a single bound
    bound_experiment_specs = [
        ('max_degree', ['enable_max_degree_bound']),
        ('sum_degree', ['enable_sum_degree_bound']),
        ('efficiency', ['enable_efficiency_bound']),
        ('packing', ['enable_packing_bound']),
        ('packing-local-search', ['enable_packing_bound', 'enable_local_search']),
        ('sum-over-packing', ['enable_packing_bound', 'enable_sum_over_packing_bound']),
        ('sum-over-packing-local-search', ['enable_packing_bound', 'enable_sum_over_packing_bound', 'enable_local_search']),
    ]
    no_bounds_settings = Settings(enable_max_degree_bound=False,
                                  enable_sum_degree_bound=False,
                                  enable_efficiency_bound=False,
                                  enable_packing_bound=False,
                                  enable_sum_over_packing_bound=False)
    for (name, enabled_settings) in bound_experiment_specs:
        settings = dataclasses.replace(
            no_bounds_settings,
            **{setting: True for setting in enabled_settings})
        add_findminhs_experiment(f'{name}-only', settings, instances)

    # Different greedy modes
    greedy_modes = ['AlwaysBeforeBounds', 'AlwaysBeforeExpensiveReductions']
    for greedy_mode in greedy_modes:
        hyphenated_mode = re.sub('([A-Z])', r'-\1', greedy_mode)[1:].lower()
        add_findminhs_experiment(f'greedy-{hyphenated_mode}',
                                 Settings(greedy_mode=greedy_mode), instances)

    run.use_cores(16)
    run.run()


if __name__ == '__main__':
    main()
