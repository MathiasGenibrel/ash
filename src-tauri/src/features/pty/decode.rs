/// Recolle l'UTF-8 découpé entre deux lectures du PTY.
///
/// Un `read()` sur un master PTY s'arrête où le tampon s'arrête, pas à une frontière de
/// caractère : un `é`, un `✓` ou un emoji peut être coupé en deux. Envoyer chaque
/// morceau tel quel afficherait des `` au milieu de la sortie, et l'utilisateur
/// n'aurait aucun moyen de savoir que c'est Ash qui a cassé son texte.
///
/// Le reliquat ne dépasse jamais trois octets — la plus longue amorce incomplète d'une
/// séquence UTF-8.
#[derive(Default)]
pub struct Utf8Stitcher {
    leftover: Vec<u8>,
}

impl Utf8Stitcher {
    /// Rend le texte complet lisible dans `bytes`, en gardant l'amorce incomplète pour
    /// la prochaine lecture.
    ///
    /// Les octets réellement invalides — un `cat` sur un binaire, par exemple — sont
    /// remplacés par `U+FFFD` plutôt que d'interrompre le flux : un terminal affiche ce
    /// qu'on lui envoie, il ne juge pas.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        let mut buffer = std::mem::take(&mut self.leftover);
        buffer.extend_from_slice(bytes);

        let mut out = String::with_capacity(buffer.len());
        let mut rest = buffer.as_slice();

        loop {
            match std::str::from_utf8(rest) {
                Ok(text) => {
                    out.push_str(text);
                    return out;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // `valid_up_to` garantit que ce préfixe est de l'UTF-8 valide.
                    out.push_str(&String::from_utf8_lossy(&rest[..valid]));

                    match error.error_len() {
                        // Octets franchement invalides : on les remplace et on continue.
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            rest = &rest[valid + bad..];
                        }
                        // Amorce incomplète : elle attend la lecture suivante.
                        None => {
                            self.leftover = rest[valid..].to_vec();
                            return out;
                        }
                    }
                }
            }
        }
    }

    /// Vide le reliquat en fin de flux.
    ///
    /// Ce qui reste ne deviendra jamais un caractère — le PTY est fermé.
    pub fn flush(&mut self) -> String {
        if self.leftover.is_empty() {
            return String::new();
        }
        let leftover = std::mem::take(&mut self.leftover);
        char::REPLACEMENT_CHARACTER
            .to_string()
            .repeat(leftover.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_multibyte_char_split_across_two_reads_when_decoding_then_it_appears_once_intact() {
        // Given — « ✓ » vaut trois octets, coupé après le premier
        let check = "✓".as_bytes();
        let mut stitcher = Utf8Stitcher::default();

        // When
        let first = stitcher.push(&[b'o', b'k', b' ', check[0]]);
        let second = stitcher.push(&[check[1], check[2], b'\n']);

        // Then
        assert_eq!(first, "ok ");
        assert_eq!(second, "✓\n");
    }

    #[test]
    fn given_a_char_split_one_byte_per_read_when_decoding_then_nothing_is_emitted_until_complete() {
        // Given — un emoji sur quatre octets, un octet par lecture
        let emoji = "🦀".as_bytes();
        let mut stitcher = Utf8Stitcher::default();

        // When
        let partial: String = emoji[..3].iter().map(|b| stitcher.push(&[*b])).collect();
        let last = stitcher.push(&[emoji[3]]);

        // Then
        assert_eq!(
            partial, "",
            "aucun caractère de remplacement pendant l'attente"
        );
        assert_eq!(last, "🦀");
    }

    #[test]
    fn given_genuinely_invalid_bytes_when_decoding_then_they_are_replaced_and_the_stream_continues()
    {
        // Given — 0xFF n'est valide dans aucune séquence UTF-8
        let mut stitcher = Utf8Stitcher::default();

        // When
        let text = stitcher.push(&[b'a', 0xFF, b'b']);

        // Then
        assert_eq!(text, "a\u{FFFD}b");
    }

    #[test]
    fn given_a_dangling_prefix_when_the_stream_ends_then_flush_surfaces_it_as_replacement() {
        // Given
        let mut stitcher = Utf8Stitcher::default();
        assert_eq!(stitcher.push(&[0xE2, 0x9C]), "");

        // When
        let tail = stitcher.flush();

        // Then
        assert_eq!(tail, "\u{FFFD}\u{FFFD}");
        assert_eq!(stitcher.flush(), "", "le reliquat n'est vidé qu'une fois");
    }
}
