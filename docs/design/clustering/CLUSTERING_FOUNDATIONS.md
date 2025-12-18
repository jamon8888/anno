# Mathematical Foundations of Clustering in Entity Resolution

*A technical reference in the tradition of careful analysis*

---

## Preamble

Entity resolution—determining which records refer to the same real-world entity—reduces at its core to **partitioning a set into equivalence classes**. This document traces the mathematical foundations of the clustering algorithms implemented in `anno-coalesce`, from their historical origins to their complexity bounds.

---

## 1. Union-Find (Disjoint-Set Union)

### Historical Origins

The disjoint-set data structure was first described by Galler and Fischer (1964) in the context of symbolic computation. Tarjan (1975) provided the definitive analysis showing that with both **path compression** and **union-by-rank**, the amortized time per operation is `O(α(n))`, where `α` is the inverse Ackermann function.

### The Data Structure

A disjoint-set forest represents a partition of elements `{1, 2, ..., n}` as a collection of rooted trees, one per equivalence class. Each element points to its parent; the root points to itself.

```
Operations:
- Find(x): Return root of tree containing x
- Union(x, y): Merge trees containing x and y
```

### Path Compression

After `Find(x)`, make every node on the path from `x` to the root point directly to the root:

```
def Find(x):
    if parent[x] ≠ x:
        parent[x] = Find(parent[x])  # Path compression
    return parent[x]
```

### Union-by-Rank

When merging two trees, attach the shallower tree under the root of the deeper tree:

```
def Union(x, y):
    rx, ry = Find(x), Find(y)
    if rx = ry: return
    if rank[rx] < rank[ry]: swap(rx, ry)
    parent[ry] = rx
    if rank[rx] = rank[ry]: rank[rx] += 1
```

### Complexity Analysis

**Theorem (Tarjan 1975).** A sequence of `m` Union and Find operations on `n` elements takes `O(m · α(n))` time, where `α` is the inverse Ackermann function.

The Ackermann function grows faster than any primitive recursive function:

```
A(0, n) = n + 1
A(m, 0) = A(m-1, 1)
A(m, n) = A(m-1, A(m, n-1))
```

Examples: A(1,1)=3, A(2,2)=7, A(3,3)=61, A(4,4) ≈ 2^(2^65536)

The inverse Ackermann function α(n) = min{k : A(k,k) ≥ n} grows so slowly that:
- α(10^80) ≤ 4 (more atoms than the observable universe)

For all practical purposes, union-find is O(m).

---

## 2. Locality-Sensitive Hashing

### The Near-Neighbor Problem

Given `n` points in a metric space and a query point `q`, find all points within distance `r` of `q`. Naive: O(n) per query. Can we do better?

### The LSH Framework (Indyk & Motwani 1998)

**Definition.** A family H of hash functions is `(d₁, d₂, p₁, p₂)`-sensitive if for any points `x, y`:
- If d(x,y) ≤ d₁: Pr[h(x) = h(y)] ≥ p₁
- If d(x,y) ≥ d₂: Pr[h(x) = h(y)] ≤ p₂

The key insight: collision probability should *decrease* with distance.

### MinHash for Jaccard Similarity

For sets A and B, the Jaccard similarity is:

```
J(A, B) = |A ∩ B| / |A ∪ B|
```

**Broder's Theorem (1997).** For a random permutation π:

```
Pr[min(π(A)) = min(π(B))] = J(A, B)
```

**Proof sketch.** Consider the union A ∪ B. The minimum element under π is equally likely to be any element. It belongs to A ∩ B exactly when both sets contain the minimum, which happens with probability |A ∩ B|/|A ∪ B|. □

### MinHash Signature

Use k independent hash functions (approximating random permutations):

```
sig(A) = (min_{a∈A} h₁(a), ..., min_{a∈A} hₖ(a))
```

The fraction of matching positions estimates J(A,B):

```
Ĵ(A,B) = |{i : sig(A)ᵢ = sig(B)ᵢ}| / k
```

By Chernoff bounds, with k = O(1/ε²) we get ε-additive approximation.

### Banding

To convert MinHash into candidate generation, group k hashes into b bands of r rows (k = br). Two items are candidates if they match in ALL rows of ANY band:

```
Pr[candidate | J(A,B) = s] = 1 - (1 - s^r)^b
```

This S-curve has a sharp transition around s* = (1/b)^(1/r), effectively filtering low-similarity pairs.

### SimHash for Cosine Similarity

For vectors, Charikar (2002) showed that random hyperplane hashing preserves cosine similarity:

```
h_r(v) = sign(r · v)

Pr[h_r(u) = h_r(v)] = 1 - θ/π
```

where θ = arccos(cos(u,v)) is the angle between u and v.

---

## 3. Correlation Clustering

### Problem Formulation

Given a complete graph G = (V, E) with edge labeling ℓ: E → {+, -}, find a partition C minimizing disagreements:

```
cost(C) = Σ_{(u,v) ∈ E⁺, C(u)≠C(v)} 1 + Σ_{(u,v) ∈ E⁻, C(u)=C(v)} 1
```

### Complexity

**Theorem (Bansal, Blum, Chawla 2004).** Correlation clustering is NP-hard.

**Proof sketch.** Reduction from MAX-CUT. Given a graph G, label all edges negative. The optimal clustering minimizes within-cluster edges, equivalent to maximizing cut edges. □

The problem is also APX-hard (no PTAS unless P=NP).

### The Pivot Algorithm

```
while unclustered vertices remain:
    pick random pivot v from unclustered
    C ← {v} ∪ {u : (v,u) ∈ E⁺ and u unclustered}
    output C as cluster
    mark all vertices in C as clustered
```

**Theorem (ACN 2008).** Pivot is a 3-approximation.

**Proof sketch.** Charge each disagreement to a unique pivot. Each positive edge between clusters is charged to the pivot that caused the separation. Each negative edge within a cluster is charged to the pivot that created that cluster. No pivot is charged more than 3 times its contribution to OPT. □

### Modified Pivot (Behnezhad et al. 2025)

The key insight: instead of adding ALL positive neighbors, use a voting scheme considering both edges:

```
Add v to cluster only if |{u ∈ C : (v,u) ∈ E⁺}| > |{u ∈ C : (v,u) ∈ E⁻}|
```

This achieves better-than-3 approximation, with ~23% fewer errors empirically.

---

## 4. Hierarchical Agglomerative Clustering

### The Lance-Williams Formula

When clusters i and j merge into (ij), the distance to any cluster k is:

```
D_{(ij),k} = αᵢDᵢₖ + αⱼDⱼₖ + βDᵢⱼ + γ|Dᵢₖ - Dⱼₖ|
```

This unified framework captures multiple linkage methods:

| Method | αᵢ | αⱼ | β | γ | Properties |
|--------|----|----|---|---|------------|
| Single | 1/2 | 1/2 | 0 | -1/2 | Chains, high recall |
| Complete | 1/2 | 1/2 | 0 | +1/2 | Compact, high precision |
| Average (UPGMA) | nᵢ/(nᵢ+nⱼ) | nⱼ/(nᵢ+nⱼ) | 0 | 0 | Balanced |
| Ward | (nᵢ+nₖ)/(nᵢ+nⱼ+nₖ) | (nⱼ+nₖ)/(nᵢ+nⱼ+nₖ) | -nₖ/(nᵢ+nⱼ+nₖ) | 0 | Variance min |

### Ward's Method

Minimizes within-cluster variance increase. The merge criterion is:

```
Δ(Cᵢ, Cⱼ) = (nᵢ · nⱼ)/(nᵢ + nⱼ) · ||μᵢ - μⱼ||²
```

where μᵢ is the centroid of cluster Cᵢ.

### Complexity

- **Naive:** O(n³) — O(n²) iterations, O(n) to find minimum each time
- **With priority queue:** O(n² log n) — O(n²) heap updates
- **Space:** O(n²) for distance matrix

---

## 5. The Streaming/Doubling Algorithm

### The Streaming Model

Documents arrive one at a time. We must:
1. Assign each to a cluster immediately
2. Use bounded memory (can't store all pairwise distances)
3. Produce reasonable clusters without seeing the future

### The Doubling Algorithm (Charikar et al. 1997)

Maintains clusters with centroids. When entity e arrives:

1. Find most similar cluster C* = argmax_C sim(e, C)
2. If sim(e, C*) ≥ θ: add e to C*
3. Else: create singleton cluster {e}

When cluster count exceeds bound, merge similar clusters.

**Theorem.** The Doubling Algorithm is an 8-approximation.

The key insight: maintain invariant that all cluster diameters are within factor 2 of each other (hence "doubling").

---

## Summary: Choosing an Algorithm

| Scenario | Algorithm | Time | Space | Guarantee |
|----------|-----------|------|-------|-----------|
| Small corpus (<10K) | Union-Find | O(n²) | O(n) | Exact |
| Large corpus (>10K) | LSH + Union-Find | O(n log n) | O(n) | ~95% recall |
| Streaming | Doubling | O(1) amort. | O(k) | 8-approx |
| Explicit ± labels | Pivot | O(n+m) | O(n+m) | 3-approx |
| Need hierarchy | HAC (Ward) | O(n² log n) | O(n²) | Exact |

---

## References

1. Galler, B.A. & Fischer, M.J. (1964). "An improved equivalence algorithm." CACM 7(5).
2. Tarjan, R.E. (1975). "Efficiency of a good but not linear set union algorithm." JACM 22(2).
3. Broder, A.Z. (1997). "On the resemblance and containment of documents." Compression and Complexity of Sequences.
4. Indyk, P. & Motwani, R. (1998). "Approximate nearest neighbors: towards removing the curse of dimensionality." STOC '98.
5. Charikar, M. (2002). "Similarity estimation techniques from rounding algorithms." STOC '02.
6. Bansal, N., Blum, A., & Chawla, S. (2004). "Correlation Clustering." Machine Learning 56(1-3).
7. Ailon, N., Charikar, M., & Newman, A. (2008). "Aggregating inconsistent information: Ranking and clustering." JACM 55(5).
8. Charikar, M., Chekuri, C., Feder, T., & Motwani, R. (1997). "Incremental clustering and dynamic information retrieval." STOC '97.
9. Lance, G.N. & Williams, W.T. (1967). "A General Theory of Classificatory Sorting Strategies." Computer Journal 9(4).
10. Ward, J.H. (1963). "Hierarchical Grouping to Optimize an Objective Function." JASA 58(301).
11. Behnezhad, S. et al. (2025). "Breaking the 3-approximation barrier for correlation clustering." ICML 2025.

