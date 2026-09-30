use std::collections::{BTreeMap, BTreeSet};

pub(in crate::compilation::product) fn dependency_order<K: Clone + Ord>(
    dependencies: &BTreeMap<K, Vec<K>>,
    order_keys: &BTreeMap<K, &[u8]>,
) -> Result<Vec<K>, Vec<K>> {
    let mut incoming = dependencies
        .keys()
        .cloned()
        .map(|key| (key, 0_usize))
        .collect::<BTreeMap<_, _>>();

    for providers in dependencies.values() {
        for provider in providers {
            let count = incoming
                .get_mut(provider)
                .expect("retained static must retain its lifecycle providers");

            *count = count
                .checked_add(1)
                .expect("static dependency count must fit usize");
        }
    }

    let mut ready = incoming
        .iter()
        .filter_map(|(key, count)| (*count == 0).then_some((order_keys[key], key.clone())))
        .collect::<BTreeSet<_>>();

    let mut ordered = Vec::with_capacity(dependencies.len());

    while let Some((_, key)) = ready.pop_first() {
        for provider in &dependencies[&key] {
            let count = incoming
                .get_mut(provider)
                .expect("static provider must have a counter");

            *count = count
                .checked_sub(1)
                .expect("static dependency edge must be consumed once");

            if *count == 0 {
                ready.insert((order_keys[provider], provider.clone()));
            }
        }

        ordered.push(key);
    }

    if ordered.len() == dependencies.len() {
        Ok(ordered)
    } else {
        Err(incoming
            .into_iter()
            .filter_map(|(key, count)| (count != 0).then_some(key))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::dependency_order;
    use std::collections::BTreeMap;

    #[test]
    fn dependencies_precede_structural_tie_breaks_and_identity_allocation() {
        let edges = BTreeMap::from([(3, vec![1]), (2, vec![]), (1, vec![])]);

        let keys = BTreeMap::from([
            (3, b"z".as_slice()),
            (2, b"b".as_slice()),
            (1, b"a".as_slice()),
        ]);

        assert_eq!(dependency_order(&edges, &keys), Ok(vec![2, 3, 1]));

        let cycle = BTreeMap::from([(3, vec![1]), (2, vec![]), (1, vec![3])]);

        assert_eq!(dependency_order(&cycle, &keys), Err(vec![1, 3]));
    }
}
