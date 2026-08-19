//! Ce qu'un fichier de police dit de lui-même : est-il à largeur fixe, et de quelle famille ?
//!
//! Le format SFNT — celui de `.ttf`, `.ttc`, `.otf` — est un annuaire de tables, chacune
//! repérée par une étiquette de quatre octets. Deux d'entre elles suffisent ici :
//!
//! - **`post`** porte `isFixedPitch`, le drapeau que le fabricant de la police pose lui-même.
//!   C'est la réponse à « est-ce une police de terminal ? », et elle est meilleure que
//!   n'importe quelle heuristique de nom : `PT Mono` est monospace, `Monotype Corsiva` ne
//!   l'est pas ;
//! - **`name`** porte les noms, dont la famille (`nameID` 1) et la famille typographique
//!   (`nameID` 16), qui la remplace quand une famille compte plus de quatre faces.
//!
//! **Rien n'est lu d'autre**, et surtout pas le fichier entier : `/System/Library/Fonts`
//! pèse plusieurs centaines de méga-octets, et le catalogue le traverse à chaque première
//! ouverture des réglages. Trois lectures courtes par fichier — l'annuaire, 16 octets de
//! `post`, la table `name` — et le reste du fichier n'est jamais touché.
//!
//! La fonction est **tolérante à tout** : un fichier tronqué, un annuaire qui pointe
//! au-delà de la fin, une table `name` sans nom lisible rendent `None`. Un catalogue de
//! polices n'est jamais une raison d'échouer — au pire, une police manque à une liste.

use std::io::{Read, Seek, SeekFrom};

/// La famille d'un fichier de police, s'il est à largeur fixe.
pub fn monospace_family<R: Read + Seek>(source: &mut R) -> Option<String> {
    let font = first_font_offset(source)?;
    let directory = table_directory(source, font)?;

    // L'ordre compte pour le coût, pas pour le résultat : `post` fait 16 octets à lire et
    // écarte la grande majorité des fichiers, `name` en fait quelques milliers.
    if !is_fixed_pitch(source, &directory)? {
        return None;
    }
    family_name(source, &directory)
}

/// Une table de l'annuaire : où elle commence, et combien elle pèse.
struct Table {
    tag: [u8; 4],
    offset: u64,
    length: usize,
}

/// Le nombre de tables d'une police tient sur un `u16`, mais en pratique il en compte moins
/// de trente. La borne évite qu'un fichier abîmé fasse allouer un mégaoctet d'annuaire.
const MAX_TABLES: usize = 512;

/// Une table `name` fait quelques kilo-octets. Au-delà, le fichier ment sur sa propre forme.
const MAX_NAME_TABLE: usize = 256 * 1024;

/// L'octet de départ de la police à lire — la première, quand le fichier en porte plusieurs.
///
/// Un `.ttc` est une **collection** : `Menlo.ttc` porte Regular, Bold, Italic et Bold Italic.
/// Les quatre appartiennent à la même famille, et c'est la famille qui est choisie dans les
/// réglages : lire la première suffit, et lire les quatre ferait quatre fois le même nom.
fn first_font_offset<R: Read + Seek>(source: &mut R) -> Option<u64> {
    let header = read_at(source, 0, 16)?;
    if &header[0..4] != b"ttcf" {
        return Some(0);
    }
    if be_u32(&header, 8)? == 0 {
        return None;
    }
    Some(u64::from(be_u32(&header, 12)?))
}

fn table_directory<R: Read + Seek>(source: &mut R, font: u64) -> Option<Vec<Table>> {
    let header = read_at(source, font, 12)?;
    let version = be_u32(&header, 0)?;
    // 0x00010000 : TrueType. `OTTO` : contours PostScript. `true` : l'ancien format macOS.
    if version != 0x0001_0000 && &header[0..4] != b"OTTO" && &header[0..4] != b"true" {
        return None;
    }

    let count = usize::from(be_u16(&header, 4)?);
    if count == 0 || count > MAX_TABLES {
        return None;
    }
    let records = read_at(source, font + 12, count * 16)?;
    let mut tables = Vec::with_capacity(count);
    for index in 0..count {
        let at = index * 16;
        tables.push(Table {
            tag: [
                *records.get(at)?,
                *records.get(at + 1)?,
                *records.get(at + 2)?,
                *records.get(at + 3)?,
            ],
            offset: u64::from(be_u32(&records, at + 8)?),
            length: be_u32(&records, at + 12)? as usize,
        });
    }
    Some(tables)
}

fn find<'a>(directory: &'a [Table], tag: &[u8; 4]) -> Option<&'a Table> {
    directory.iter().find(|table| &table.tag == tag)
}

/// Le drapeau que le fabricant de la police pose : toutes ses gravures ont la même chasse.
fn is_fixed_pitch<R: Read + Seek>(source: &mut R, directory: &[Table]) -> Option<bool> {
    let post = find(directory, b"post")?;
    // Version, angle d'italique, position et épaisseur du soulignement, puis le drapeau :
    // 16 octets suffisent, quelle que soit la version de la table.
    let head = read_at(source, post.offset, 16)?;
    Some(be_u32(&head, 12)? != 0)
}

/// Le nom de famille, tiré de la table `name`.
fn family_name<R: Read + Seek>(source: &mut R, directory: &[Table]) -> Option<String> {
    let name = find(directory, b"name")?;
    if name.length == 0 || name.length > MAX_NAME_TABLE {
        return None;
    }
    let table = read_at(source, name.offset, name.length)?;

    let count = usize::from(be_u16(&table, 2)?);
    let strings = usize::from(be_u16(&table, 4)?);
    let mut best: Option<(u8, String)> = None;
    for index in 0..count {
        let at = 6 + index * 12;
        let platform = be_u16(&table, at)?;
        let encoding = be_u16(&table, at + 2)?;
        let name_id = be_u16(&table, at + 6)?;
        let length = usize::from(be_u16(&table, at + 8)?);
        let offset = usize::from(be_u16(&table, at + 10)?);

        let Some(rank) = rank(platform, name_id) else {
            continue;
        };
        if best.as_ref().is_some_and(|(kept, _)| *kept >= rank) {
            continue;
        }
        let start = strings.checked_add(offset)?;
        let Some(bytes) = table.get(start..start.checked_add(length)?) else {
            continue;
        };
        let Some(decoded) = decode(platform, encoding, bytes) else {
            continue;
        };
        if !decoded.trim().is_empty() {
            best = Some((rank, decoded.trim().to_owned()));
        }
    }
    best.map(|(_, family)| family)
}

/// Ce qu'on préfère lire, du meilleur au moins bon — `None` quand l'enregistrement ne dit
/// pas une famille.
///
/// `nameID` 16 passe avant 1 : c'est la famille **typographique**, celle qui dit
/// « JetBrains Mono » là où `nameID` 1 dit « JetBrains Mono Light » — une famille de plus de
/// quatre faces se découpe en sous-familles dans les vieux noms, et un menu déroulant les
/// listerait toutes séparément. La plateforme 3 (Windows) passe avant la 1 (Mac) : c'est
/// celle dont l'encodage est de l'Unicode, donc la seule qui rende les noms accentués.
fn rank(platform: u16, name_id: u16) -> Option<u8> {
    match (name_id, platform) {
        (16, 3) => Some(4),
        (16, _) => Some(3),
        (1, 3) => Some(2),
        (1, _) => Some(1),
        _ => None,
    }
}

fn decode(platform: u16, encoding: u16, bytes: &[u8]) -> Option<String> {
    // Plateforme 1 (Mac), encodage 0 : du Mac Roman, dont les 128 premiers points de code
    // sont de l'ASCII. Les noms de famille au-delà sont rares, et un nom à moitié traduit
    // vaudrait moins que pas de nom du tout.
    if platform == 1 && encoding == 0 {
        return bytes
            .is_ascii()
            .then(|| String::from_utf8_lossy(bytes).into_owned());
    }
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

/// Lit exactement `length` octets à `offset`, ou rien.
///
/// `read_exact` et non `read` : une lecture courte au bord du fichier rendrait une table
/// tronquée qu'on interpréterait quand même.
fn read_at<R: Read + Seek>(source: &mut R, offset: u64, length: usize) -> Option<Vec<u8>> {
    source.seek(SeekFrom::Start(offset)).ok()?;
    let mut buffer = vec![0_u8; length];
    source.read_exact(&mut buffer).ok()?;
    Some(buffer)
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Cursor;

    /// Un fichier de police, construit octet par octet.
    ///
    /// Il est ici plutôt qu'à côté d'un `.ttf` d'exemple pour deux raisons : le dépôt ne
    /// versionne pas de binaire, et surtout un fichier réel ne se **fait pas varier** — c'est
    /// exactement le drapeau `isFixedPitch` et la forme de la table `name` qu'on veut pouvoir
    /// changer d'un test à l'autre. Ses défauts sont valides : deux tables, un nom Windows.
    pub struct FontFileBuilder {
        family: String,
        fixed_pitch: bool,
        collection: bool,
        /// La plateforme et l'encodage de l'enregistrement de nom — Windows/Unicode par défaut.
        name_records: Vec<(u16, u16, u16, String)>,
    }

    impl FontFileBuilder {
        pub fn new(family: &str) -> Self {
            Self {
                family: family.to_owned(),
                fixed_pitch: true,
                collection: false,
                name_records: Vec::new(),
            }
        }

        pub fn fixed_pitch(mut self, fixed: bool) -> Self {
            self.fixed_pitch = fixed;
            self
        }

        /// Enveloppe la police dans une collection `ttcf`, comme `Menlo.ttc`.
        pub fn collection(mut self) -> Self {
            self.collection = true;
            self
        }

        /// Un enregistrement de nom de plus : plateforme, encodage, `nameID`, valeur.
        pub fn name(mut self, platform: u16, encoding: u16, name_id: u16, value: &str) -> Self {
            self.name_records
                .push((platform, encoding, name_id, value.to_owned()));
            self
        }

        pub fn build(self) -> Vec<u8> {
            let records = if self.name_records.is_empty() {
                vec![(3_u16, 1_u16, 1_u16, self.family.clone())]
            } else {
                self.name_records.clone()
            };
            let name_table = name_table(&records);
            let mut post_table = vec![0_u8; 32];
            post_table[12..16]
                .copy_from_slice(&(if self.fixed_pitch { 1_u32 } else { 0 }).to_be_bytes());

            let prefix = if self.collection { 16 } else { 0 };
            let directory_end = prefix + 12 + 2 * 16;
            let name_offset = directory_end;
            let post_offset = name_offset + name_table.len();

            let mut file = Vec::new();
            if self.collection {
                file.extend_from_slice(b"ttcf");
                file.extend_from_slice(&1_u32.to_be_bytes());
                file.extend_from_slice(&1_u32.to_be_bytes());
                file.extend_from_slice(&16_u32.to_be_bytes());
            }
            file.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
            file.extend_from_slice(&2_u16.to_be_bytes());
            file.extend_from_slice(&[0; 6]);
            for (tag, offset, length) in [
                (b"name", name_offset, name_table.len()),
                (b"post", post_offset, post_table.len()),
            ] {
                file.extend_from_slice(tag);
                file.extend_from_slice(&0_u32.to_be_bytes());
                file.extend_from_slice(&(offset as u32).to_be_bytes());
                file.extend_from_slice(&(length as u32).to_be_bytes());
            }
            file.extend_from_slice(&name_table);
            file.extend_from_slice(&post_table);
            file
        }
    }

    fn name_table(records: &[(u16, u16, u16, String)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut offsets = Vec::new();
        for (platform, encoding, _, value) in records {
            let encoded: Vec<u8> = if *platform == 1 && *encoding == 0 {
                value.bytes().collect()
            } else {
                value.encode_utf16().flat_map(u16::to_be_bytes).collect()
            };
            offsets.push((strings.len(), encoded.len()));
            strings.extend_from_slice(&encoded);
        }

        let string_offset = 6 + records.len() * 12;
        let mut table = Vec::new();
        table.extend_from_slice(&0_u16.to_be_bytes());
        table.extend_from_slice(&(records.len() as u16).to_be_bytes());
        table.extend_from_slice(&(string_offset as u16).to_be_bytes());
        for ((platform, encoding, name_id, _), (offset, length)) in records.iter().zip(offsets) {
            table.extend_from_slice(&platform.to_be_bytes());
            table.extend_from_slice(&encoding.to_be_bytes());
            table.extend_from_slice(&0_u16.to_be_bytes());
            table.extend_from_slice(&name_id.to_be_bytes());
            table.extend_from_slice(&(length as u16).to_be_bytes());
            table.extend_from_slice(&(offset as u16).to_be_bytes());
        }
        table.extend_from_slice(&strings);
        table
    }

    fn read(bytes: Vec<u8>) -> Option<String> {
        monospace_family(&mut Cursor::new(bytes))
    }

    #[test]
    fn given_a_fixed_pitch_font_when_it_is_read_then_its_family_is_offered() {
        // Given — le cas nominal : c'est le fabricant de la police qui déclare la chasse
        // fixe, et c'est cette déclaration qu'on lit plutôt que de deviner sur le nom
        let file = FontFileBuilder::new("Iosevka Term").build();

        // When / Then
        assert_eq!(read(file), Some("Iosevka Term".to_owned()));
    }

    #[test]
    fn given_a_proportional_font_when_it_is_read_then_it_is_not_offered_for_a_terminal() {
        // Given — `Monotype Corsiva` porte « Mono » dans son nom et n'aligne rien : c'est
        // exactement la faute qu'une table de noms aurait faite
        let file = FontFileBuilder::new("Monotype Corsiva")
            .fixed_pitch(false)
            .build();

        // When / Then
        assert_eq!(read(file), None);
    }

    #[test]
    fn given_a_font_that_names_both_a_family_and_a_typographic_family_when_it_is_read_then_the_menu_gets_the_shorter_one(
    ) {
        // Given — une famille de plus de quatre faces se découpe dans les vieux noms
        // (`nameID` 1), et un menu déroulant listerait alors `JetBrains Mono Light`,
        // `JetBrains Mono ExtraBold`… au lieu d'une entrée
        let file = FontFileBuilder::new("ignoré")
            .name(3, 1, 1, "JetBrains Mono Light")
            .name(3, 1, 16, "JetBrains Mono")
            .build();

        // When / Then
        assert_eq!(read(file), Some("JetBrains Mono".to_owned()));
    }

    #[test]
    fn given_a_font_named_only_the_old_macintosh_way_when_it_is_read_then_its_family_still_comes_through(
    ) {
        // Given — les polices livrées avec macOS depuis longtemps n'ont parfois que cet
        // enregistrement, et les laisser tomber retirerait `Monaco` de la liste
        let file = FontFileBuilder::new("ignoré")
            .name(1, 0, 1, "Monaco")
            .build();

        // When / Then
        assert_eq!(read(file), Some("Monaco".to_owned()));
    }

    #[test]
    fn given_a_collection_holding_the_four_faces_of_one_family_when_it_is_read_then_it_names_that_family_once(
    ) {
        // Given — `Menlo.ttc` : quatre polices dans un fichier, une seule famille
        let file = FontFileBuilder::new("Menlo").collection().build();

        // When / Then
        assert_eq!(read(file), Some("Menlo".to_owned()));
    }

    #[test]
    fn given_a_file_that_is_not_a_font_or_that_stops_short_when_it_is_read_then_nothing_is_guessed()
    {
        // Given — un `.ttf` tronqué par une copie interrompue, un fichier texte renommé, et
        // un annuaire qui pointe au-delà de la fin : le catalogue traverse des dossiers
        // qu'Ash n'a pas écrits, et rien n'y est garanti
        let complete = FontFileBuilder::new("Iosevka").build();
        let broken = vec![
            Vec::new(),
            b"ceci n'est pas une police du tout".to_vec(),
            complete[..complete.len() - 20].to_vec(),
        ];

        // When
        let read_back: Vec<Option<String>> = broken.into_iter().map(read).collect();

        // Then
        assert_eq!(read_back, vec![None, None, None]);
    }
}
