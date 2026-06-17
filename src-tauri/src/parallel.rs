use rayon::prelude::*;

pub(crate) fn ordered_map<T, U, E, F>(items: &[T], map: F) -> Result<Vec<U>, E>
where
    T: Sync,
    U: Send,
    E: Send,
    F: Fn(usize, &T) -> Result<U, E> + Send + Sync,
{
    let mut indexed = items
        .par_iter()
        .enumerate()
        .map(|(index, item)| (index, map(index, item)))
        .collect::<Vec<_>>();
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}
