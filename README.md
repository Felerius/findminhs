# An Efficient Branch-and-Bound Solver for Hitting Set

Implementation of the Minimum Hitting Set solver described in the [An Efficient Branch-and-Bound
Solver for Hitting Set research paper][paper]. Also included is the code used for the evaluation
section of the paper.

*Note*: Although the solver is fundamentally the same, a few features have been added compared to
the version described in the paper. For reproducing the results from the paper or my earlier master
thesis please refer to the Github releases and their respective git tags.

## Installing

On the Github release pages you can download binaries for Linux, macOS, and Windows. If you happen
to have the [Cargo package manager][cargo] installed, you can use it to install findmins from
[crates.io][crates.io] using `cargo install findmins`. Lastly, you can of course build it yourself,
using a recent version of [Rust][rust]. To get started, `cargo build --release` in a checkout of
this repository will create an optimized binary in the `target/release` directory.

## Usage

To run the solver use `findminhs solve <hypergraph-file> <settings-file>`. The formats for both
files are described below. You can pass `-s/--solution <file>` to write the final hitting set to a
file formatted as a JSON array. Similarly, `-r/--report <file>` can be used to write a JSON
formatted report containing statistics about the solving process. For all further details, refer to
the included help messages using `-h/--help`.

### Hypergraph format

The solver accepts hypergraphs in two formats: in JSON and in a custom, text-based format. The
latter is the default while the former can be enabled by passing `-j/--json`.

The text-based format must start with an initial line containing the number of vertices followed by
the number of hyperedges. It must then contain one line per hyperedge. Each line must first contain
the size of the hyperedge followed by the zero-based indices of the nodes contained in the
hyperedge, in arbitrary order. As an example, the hypergraph of four vertices and the two hyperedges
{0, 1, 2} and {2, 3} could be encoded as such:

```text
4 2
3 0 1 2
2 2 3
```

The JSON format only contains the number of nodes as well as an array of hyperedges, each
represented as an array. The hypergraph from above could be encoded as

```json
{
  "num_nodes": 4,
  "edges": [
    [0, 1, 2],
    [2, 3]
  ]
}
```

### Settings format

The settings file is a JSON file in the same format as this example:

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

Refer to the [paper][paper] for a detailed description of these options. The above example
represents the default settings we used in the paper. The possible values for `greedy_mode` are:
`Never`, `Once`, `AlwaysBeforeBounds`, and `AlwaysBeforeExpensiveReductions`.

Additionally, there are two optional settings that can be used. The first, `initial_hitting_set`,
initializes the solver with a given hitting set. It must be specified as an array containing
zero-based node indices. The second is `stop_at`, which must be given an integer value. It instructs
the solver to stop once a hitting set of the given size or smaller is found. These can be used to
speed up the solver in situations where finding a minimum hitting set is not the objective, for
example when verifying that a given hitting set is minimum.

## Evaluation

The code for the evaluation section of the [paper][paper] is in the [`evaluation`](evaluation)
directory. Refer to its readme for details.

[paper]: https://epubs.siam.org/doi/10.1137/1.9781611977042.17
[cargo]: https://doc.rust-lang.org/stable/cargo/
[crates.io]: https://crates.io/
[rust]: https://rust-lang.org
