# An Efficient Branch-and-Bound Solver for Hitting Set

Implementation of a Minimum Hitting Set solver described in the An Efficient Branch-and-Bound Solver for Hitting Set research paper.
Also included is the code used for the evaluation section of the paper.

*Note*: In the Github releases you can find the earlier version of this solver described in my master thesis.

## Building

The solver is implemented in the [Rust programming language](https://rust-lang.org) using the Cargo project manager included with it.
All dependencies are listed in the project format and will be downloaded automatically.
To get started, `cargo build --release` creates an optimized build in the `target/release` directory.

## Usage

To run the solver use `./findminhs solve <hypergraph-file> <settings-file>`.
The formats for both inputs are described in detail below.
Pass `-r <report-file>` to have the solver generate a json formatted report containing statistics about the solving process.
For all further details, refer to the included help messages in the command-line interface.

### Hypergraph format

The solver expects hypergraphs in the following text-based format: an initial line containing the number of vertices and hyperedges followed by one line per hyperedge.
Each hyperedge line should contain the number of vertices in it followed by the zero-based indices of the vertices.

The following file represents a hypergraph of three vertices and two hyperedges {0, 1} and {1, 2}:

```text
3 2
2 0 1
2 1 2
```

### Settings format

The settings file is a json file in the same format as this example:

```json
{
    "enable_local_search": false,
    "enable_max_degree_bound": true,
    "enable_sum_degree_bound": false,
    "enable_efficiency_bound": true,
    "enable_packing_bound": true,
    "enable_sum_over_packing_bound": true,
    "packing_from_scratch_limit": 3,
    "greedy_mode": "Once"
}
```

The possible values for `greedy_mode` are: `Never`, `Once`, `AlwaysBeforeBounds`, and `AlwaysBeforeExpensiveReductions`.

## Evaluation

The code for the evaluation is in the [`evaluation`](evaluation) directory.
Refer to its readme for details.
