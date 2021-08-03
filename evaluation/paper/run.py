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
ILP_DIR = SCRIPT_DIR / 'ilps'
ILP_REDUCED_DIR = SCRIPT_DIR / 'ilps-reduced'
ILP_REDUCTION_REPORT_DIR = SCRIPT_DIR / 'reduction-logs'
GUROBI_LOGS_DIR = SCRIPT_DIR / 'gurobi-logs'
GUROBI_REDUCED_LOGS_DIR = SCRIPT_DIR / 'gurobi-reduced-logs'
GUROBI_SOLUTIONS_DIR = SCRIPT_DIR / 'gurobi-solutions'
GUROBI_REDUCED_SOLUTIONS_DIR = SCRIPT_DIR / 'gurobi-reduced-solutions'
GUROBI_EXE = 'gurobi_cl'

TIMEOUT = '24h'


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


def main() -> None:
    instances = [f.stem for f in INSTANCE_DIR.glob('*.dat')]
    experiments = []

    def add_experiment(name: str, settings: Settings) -> None:
        results_dir = RESULTS_DIR / name
        logs_dir = LOGS_DIR / name
        settings_file = SETTINGS_DIR / (name + '.json')
        for directory in (results_dir, logs_dir, SETTINGS_DIR):
            directory.mkdir(exist_ok=True, parents=True)
        with settings_file.open('w') as f:
            json.dump(dataclasses.asdict(settings), f)
        experiments.append(name)

    run.group('all')

    # All default settings
    add_experiment('default', Settings())

    # Different packing from scratch limits
    for i in [0, 1, 5, 7, 9, 11, 13, 15, 17, 19]:
        add_experiment(
            f'from-scratch-{i}',
            Settings(packing_from_scratch_limit=i),
        )

    # Different experiments using only a single bound
    bound_experiment_specs = [
        ('max-degree', ['enable_max_degree_bound']),
        ('sum-degree', ['enable_sum_degree_bound']),
        ('efficiency', ['enable_efficiency_bound']),
        ('packing', ['enable_packing_bound']),
        ('packing-local-search',
         ['enable_packing_bound', 'enable_local_search']),
        ('sum-over-packing',
         ['enable_packing_bound', 'enable_sum_over_packing_bound']),
        ('sum-over-packing-local-search', [
            'enable_packing_bound', 'enable_sum_over_packing_bound',
            'enable_local_search'
        ]),
    ]
    no_bounds_settings = Settings(enable_max_degree_bound=False,
                                  enable_sum_degree_bound=False,
                                  enable_efficiency_bound=False,
                                  enable_packing_bound=False,
                                  enable_sum_over_packing_bound=False)
    for (name, enabled_settings) in bound_experiment_specs:
        settings = dataclasses.replace(
            no_bounds_settings,
            **{setting: True
               for setting in enabled_settings})
        add_experiment(f'{name}-only', settings)

    # Different greedy modes
    greedy_modes = [
        'Never', 'AlwaysBeforeBounds', 'AlwaysBeforeExpensiveReductions'
    ]
    for greedy_mode in greedy_modes:
        hyphenated_mode = re.sub('([A-Z])', r'-\1', greedy_mode)[1:].lower()
        add_experiment(f'greedy-{hyphenated_mode}',
                       Settings(greedy_mode=greedy_mode))

    # Generate combined experiment for all findminhs runs. This avoids
    # blocking on long-running instances of a single experiment, which
    # would leave lots of cores unused.
    findminhs_command = (
        f'timeout {TIMEOUT} {SCRIPT_DIR}/findminhs solve '
        f'{INSTANCE_DIR}/[[name]].dat '
        f'{SETTINGS_DIR}/[[experiment]].json '
        f'--report {RESULTS_DIR}/[[experiment]]/[[name]].json 2>&1')
    run.add(
        'findminhs',
        findminhs_command,
        {
            'name': instances,
            'experiment': experiments
        },
        stdout_file=f'{LOGS_DIR}/[[experiment]]/[[name]].log',
        creates_file=f'{RESULTS_DIR}/[[experiment]]/[[name]].json',
        allowed_return_codes=[0, 124],
    )

    # Generate ILP files
    ILP_DIR.mkdir(exist_ok=True, parents=True)
    run.add('generate-ilps',
            f'{SCRIPT_DIR}/findminhs ilp {INSTANCE_DIR}/[[name]].dat',
            {'name': instances},
            stdout_file=f'{ILP_DIR}/[[name]].ilp',
            creates_file=f'{ILP_DIR}/[[name]].ilp')

    # Generate pre-reduced ILP files
    ILP_REDUCED_DIR.mkdir(exist_ok=True, parents=True)
    ILP_REDUCTION_REPORT_DIR.mkdir(exist_ok=True, parents=True)
    reduced_ilp_cmd = f'''
        {SCRIPT_DIR}/findminhs ilp --reduced --report
        {ILP_REDUCTION_REPORT_DIR}/[[name]].json {INSTANCE_DIR}/[[name]].dat
    '''.replace('\n', ' ')
    run.add('generate-reduced-ilps',
            reduced_ilp_cmd, {'name': instances},
            stdout_file=f'{ILP_REDUCED_DIR}/[[name]].ilp',
            creates_file=f'{ILP_REDUCED_DIR}/[[name]].ilp')

    # Solve with Gurobi
    for reduced in (False, True):
        logs_dir = GUROBI_REDUCED_LOGS_DIR if reduced else GUROBI_LOGS_DIR
        solutions_dir = GUROBI_REDUCED_SOLUTIONS_DIR if reduced else GUROBI_SOLUTIONS_DIR
        ilp_dir = ILP_REDUCED_DIR if reduced else ILP_DIR

        logs_dir.mkdir(exist_ok=True, parents=True)
        solutions_dir.mkdir(exist_ok=True, parents=True)

        gurobi_cmd = f'''
            timeout {TIMEOUT} {GUROBI_EXE} Threads=1
            LogFile={logs_dir}/[[name]].log
            ResultFile={solutions_dir}/[[name]].sol
            {ilp_dir}/[[name]].ilp
        '''.replace('\n', ' ')
        run.add(
            'gurobi-reduced' if reduced else 'gurobi',
            gurobi_cmd,
            {'name': instances},
            creates_file=f'{solutions_dir}/[[name]].sol',
            allowed_return_codes=[0, 124],
        )

    run.use_cores(124)
    run.run()


if __name__ == '__main__':
    main()
