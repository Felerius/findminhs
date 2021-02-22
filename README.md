# Activity-Based Minimum Hitting Set Solver

Implementation of the Minimum Hitting Set solver described in my master thesis.
It uses a branching heuristic based on the popular VSIDS heuristic used in SAT solvers, introduced by Moskewicz et al. [[Mos+01]](https://dl.acm.org/doi/abs/10.1145/378239.379017).
Also included are the results used in the evaluation part of the thesis, as well as the code used to evaluate them.

## Building

The solver is implemented using the Rust programming language.
It is structured as a Cargo project, so you can use the Cargo project manager included with Rust to build it.
All required dependencies will be downloaded automatically.
For example, `cargo build` would build a debug version of the solver.

The Cargo project uses several features to control which version of the activity heuristic is used (including disabling it).
To replicate the settings used in the thesis, pass `--features=branching-random` to Cargo to build with uniform-random branching, or `--features=branching-activity` for the `abs-incl` heuristic.

## Usage

Pass the `--help` flag to the solver to get a list of options (for example with `cargo run -- --help`).
To collect results, you can pass a path to the CSV file to the solver using `-c`.
Once the solver finishes, it appends an entry to this file (or creates it, if it doesn't exist).

## Hypergraph Format

The solver expects hypergraphs in a text based format.
The first line of file should contain two integers: first the number of vertices, then the hyperedges.
The following lines each represent a hyperedge, containing firstly the number of vertices in the edge, and then the zero-based vertex identifiers.

The following file would represent a hypergraph of three vertices and and the two hyperedges {0, 1} and {1, 2}:

```text
3 2
2 0 1
2 1 2
```

## Evaluation

The `evaluation` directory contains the Jupyter notebook used to evaluate the experimental results for my thesis.
The `evaluation/results` directory contains the results of the experimental runs, both in form of the CSV file produced by the solver as well as logs of its output.
The CSV files include the random number generator seed used for each run.
Note that the experiments were run in parallel, so the logs contain interleaved output of parallel runs.
