## Cellular Automata State Space Search

<ins>Objective</ins>: To build a cellular automata state space search program, which will search through all possible rule combinations for all possible cells to generate new cell.

<ins>Space</ins>: Start with one cell, apply rules. If successful, the applied rules would produce prime numbers :D

<ins>Cell</ins>: N-dimensional vector (N specified at compile time) + Value. Value is either Set or Unset

<ins>Rule</ins>:

One rule specifies which neighbours to check, what check to be done and what rule should be applied.

Can there be multiple rules? - RuleSet. Order of rule applicability

Which neighbours to check -  This is defined by the "elements" vector, where each element has 3 values: SameCoordinate, Positive, Negative
As expected of the naming, when this rule is checked against a cell, it'll check it's neighbours, changing ith coordinate accordingly (SameCoordinate - no change, Positive - +1, Negative - -1)

What check to be done – Is whether the specified neighbours are set or not set. Rule has ExpectedCellValue which checks if the neighbours are as expected

What rule should be applied – Is whether the corresponding cell should be set, unset or flipped in value. RuleResult does this.


<ins>Dimensions</ins>:

Number of dimensions of the universe is unknown. We need to generate for increasing numbers, starting at 3 (since we live in 3 dimensions)


State space search:
Stop the search if successive applications of rule is not producing the next prime number in sequence (2,3,5,7,11,13...)