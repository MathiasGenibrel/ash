//! Ce qui est réellement transverse, et rien d'autre.
//!
//! Un module n'entre ici que s'il sert **au moins deux features** et ne porte **aucune
//! règle** propre à l'une d'elles (voir `.claude/docs/architecture.md`). Le temps remplit
//! les deux conditions : lire l'heure est un effet système, pas une règle de git ni une
//! règle d'agent.

pub mod time;
