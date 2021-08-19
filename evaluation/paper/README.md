# Evaluation

This directory contains the code for the evaluation section of the paper.
Detailed below are the steps to reproduce our results.

## Building

Run `./build.sh` to build the solver and copy it to this directory.
This uses version 1.53 of Rust to build the solver as well as the `x86_64-unknown-linux-gnu` target.
Make sure to have correct Rust version and the standard library and liker for this target installed.

## Running the Experiments

Extract the instances into an `instances` directory in this folder.
Then use `python3 run.py all` to run all experiments (Python version 3.6 or later is required).
The root of this repository contains a `Pipfile` (which can be used using [`pipenv`](https://pipenv.pypa.io/en/latest/)) containing the dependencies both for this step and the next.
Additionally, Gurobi's command line tool `gurobi_cl` must be available for the Gurobi experiments.

You may want to adjust the time limit and number of cores used inside `run.py`.

## Evaluating

Use the `evalution.ipynb` Jupyter notebook to evaluate the results.
See the previous section for details on how to install the Python dependencies for this step.
