# Evaluation

This directory contains the code for the evaluation section of the paper.
Detailed below are the steps to reproduce our results.

## Building

Run `./build.sh` to build the solver and copy it to this directory.
This uses version 1.53 of Rust to build the solver for the `x86_64-unknown-linux-gnu` target.
Make sure you have Rust 1.53 as well as the standard library and a linker for this target installed.

## Running the Experiments

An archive containing all the hitting set instances used can be found in the Github release.
Extract it into an `instances` directory in this folder, then use `python3 run.py all` to run all experiments.
This requires Python 3.6 or later as well as the dependencies listed in the `Pipfile` in the root of this repository.
The latter can be installed using [`pipenv`](https://pipenv.pypa.io/en/latest/).
Additionally, Gurobi's command line tool `gurobi_cl` must be available for the Gurobi experiments.
We used Gurobi 9.1.2 in our experiments.

You may want to adjust the time limit and number of cores used inside `run.py`.

## Evaluating

Use the `evalution.ipynb` [Jupyter](https://jupyter.org/) notebook to evaluate the results.
See the previous section for details on how to install the Python dependencies for this step.
