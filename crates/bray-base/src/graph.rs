use std::collections::{BTreeMap, BTreeSet};

/// Partitions a directed graph into strongly connected components without recursive traversal.
///
/// Roots and their reachable successors form the graph. Components and members follow the supplied
/// root and successor order. Vertices may be typed identities or storage indices.
pub fn strongly_connected_components<N, I>(
    roots: impl IntoIterator<Item = N>,
    successors: impl Fn(N) -> I,
) -> Vec<Vec<N>>
where
    N: Copy + Ord,
    I: IntoIterator<Item = N>,
{
    let mut visited = BTreeSet::new();
    let mut reverse = BTreeMap::<N, Vec<N>>::new();
    let mut order = Vec::new();

    for root in roots {
        if !visited.insert(root) {
            continue;
        }

        let mut stack = vec![(root, successors(root).into_iter())];

        while let Some((node, children)) = stack.last_mut() {
            if let Some(successor) = children.next() {
                reverse.entry(successor).or_default().push(*node);

                if visited.insert(successor) {
                    stack.push((successor, successors(successor).into_iter()));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }

    visited.clear();
    let mut components = Vec::new();

    for root in order.into_iter().rev() {
        if !visited.insert(root) {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            component.push(node);

            for predecessor in reverse.get(&node).into_iter().flatten() {
                if visited.insert(*predecessor) {
                    stack.push(*predecessor);
                }
            }
        }

        components.push(component);
    }

    components
}

#[cfg(test)]
mod tests {
    use super::strongly_connected_components;
    use std::collections::BTreeMap;

    #[test]
    fn components_include_cycles_leaves_and_disconnected_vertices() {
        let edges = BTreeMap::from([
            (0, vec![1]),
            (1, vec![0, 2]),
            (2, vec![3]),
            (3, vec![2]),
            (4, vec![4]),
            (5, vec![6]),
        ]);

        let mut components = strongly_connected_components(edges.keys().copied(), |node| {
            edges.get(&node).into_iter().flatten().copied()
        });

        for component in &mut components {
            component.sort_unstable();
        }

        components.sort_unstable();

        assert_eq!(
            components,
            vec![vec![0, 1], vec![2, 3], vec![4], vec![5], vec![6]]
        );
    }

    #[test]
    fn deep_graphs_use_an_explicit_traversal_stack() {
        let components =
            strongly_connected_components([0u32], |node| (node < 20_000).then_some(node + 1));

        assert_eq!(components.len(), 20_001);
        assert!(components.iter().all(|component| component.len() == 1));
    }
}
