//! Les implémentations du trait [`Adapter`](super::adapter::Adapter), une par outil.
//!
//! Ajouter un outil, c'est ajouter un fichier ici et le déclarer à la composition root —
//! sans toucher au cœur ni à l'interface
//! ([ADR-0008](../../../../../docs/adr/0008-abstraction-adapter.md)). Chacune passe la
//! suite contractuelle de [`super::contract`], puis teste ce qui lui est propre.

mod generic;

pub use generic::GenericAdapter;
