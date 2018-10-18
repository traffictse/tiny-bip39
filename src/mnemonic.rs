use util::{truncate, checksum, BitWriter, BitReader, Bits11};
use crypto::{gen_random_bytes, sha256};
use error::{ErrorKind, Result};
use mnemonic_type::MnemonicType;
use language::Language;
use std::fmt;

/// The primary type in this crate, most tasks require creating or using one.
///
/// To create a *new* [`Mnemonic`][Mnemonic] from a randomly generated key, call [`Mnemonic::new()`][Mnemonic::new()].
///
/// To get a [`Mnemonic`][Mnemonic] instance for an existing mnemonic phrase, including
/// those generated by other software or hardware wallets, use [`Mnemonic::from_phrase()`][Mnemonic::from_phrase()].
///
/// You can get the HD wallet [`Seed`][Seed] from a [`Mnemonic`][Mnemonic] by calling [`Seed::new()`][Seed::new()].
/// From there you can either get the raw byte value with [`Seed::as_bytes()`][Seed::as_bytes()], or the hex
/// representation using Rust formatting: `format!("{:X}", seed)`.
///
/// You can also get the original entropy value back from a [`Mnemonic`][Mnemonic] with [`Mnemonic::entropy()`][Mnemonic::entropy()],
/// but beware that the entropy value is **not the same thing** as an HD wallet seed, and should
/// *never* be used that way.
///
/// [Mnemonic]: ./mnemonic/struct.Mnemonic.html
/// [Mnemonic::new()]: ./mnemonic/struct.Mnemonic.html#method.new
/// [Mnemonic::from_phrase()]: ./mnemonic/struct.Mnemonic.html#method.from_phrase
/// [Mnemonic::entropy()]: ./mnemonic/struct.Mnemonic.html#method.entropy
/// [Seed]: ./seed/struct.Seed.html
/// [Seed::new()]: ./seed/struct.Seed.html#method.new
/// [Seed::as_bytes()]: ./seed/struct.Seed.html#method.as_bytes
///
#[derive(Clone)]
pub struct Mnemonic {
    phrase: String,
    lang: Language,
    entropy: Vec<u8>,
}

impl Mnemonic {
    /// Generates a new [`Mnemonic`][Mnemonic]
    ///
    /// Use [`Mnemonic::phrase()`][Mnemonic::phrase()] to get an `str` slice of the generated phrase.
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, MnemonicType, Language};
    ///
    /// let mnemonic = Mnemonic::new(MnemonicType::Words12, Language::English);
    /// let phrase = mnemonic.phrase();
    ///
    /// println!("phrase: {}", phrase);
    ///
    /// assert_eq!(phrase.split(" ").count(), 12);
    /// ```
    ///
    /// [Mnemonic]: ./mnemonic/struct.Mnemonic.html
    /// [Mnemonic::phrase()]: ./mnemonic/struct.Mnemonic.html#method.phrase
    pub fn new(mtype: MnemonicType, lang: Language) -> Mnemonic {
        let entropy = gen_random_bytes(mtype.entropy_bits() / 8);

        Mnemonic::from_entropy_unchecked(entropy, lang)
    }

    /// Create a [`Mnemonic`][Mnemonic] from pre-generated entropy
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, MnemonicType, Language};
    ///
    /// let entropy = &[0x33, 0xE4, 0x6B, 0xB1, 0x3A, 0x74, 0x6E, 0xA4, 0x1C, 0xDD, 0xE4, 0x5C, 0x90, 0x84, 0x6A, 0x79];
    /// let mnemonic = Mnemonic::from_entropy(entropy, Language::English).unwrap();
    ///
    /// assert_eq!("crop cash unable insane eight faith inflict route frame loud box vibrant", mnemonic.phrase());
    /// assert_eq!("33E46BB13A746EA41CDDE45C90846A79", format!("{:X}", mnemonic));
    /// ```
    ///
    /// [Mnemonic]: ../mnemonic/struct.Mnemonic.html
    pub fn from_entropy(entropy: &[u8], lang: Language) -> Result<Mnemonic> {
        // Validate entropy size
        MnemonicType::for_key_size(entropy.len() * 8)?;

        Ok(Self::from_entropy_unchecked(entropy, lang))
    }

    fn from_entropy_unchecked<E>(entropy: E, lang: Language) -> Mnemonic
    where
        E: Into<Vec<u8>>
    {
        let entropy = entropy.into();
        let wordlist = lang.wordlist();

        let checksum_byte = sha256(&entropy).as_ref()[0];

        let phrase = {
            // First, create a byte iterator for the given entropy and the first byte of the
            // hash of the entropy that will serve as the checksum (up to 8 bits for biggest
            // entropy source).
            let mut iter = entropy.iter().cloned().chain(Some(checksum_byte));

            // Then we transform that into a BitReader iterator, that returns 11 bits at a
            // time (as u16), which we can map to the words on the `wordlist`.
            //
            // Assuming the entropy size is correct, this ought to give us the correct amount
            // of words.
            let mut words = BitReader::new(iter, Bits11).map(|n| wordlist[n as usize]);
            let mut phrase = String::with_capacity(128);

            phrase.push_str(words.next().expect("Must have at least one word; qed"));

            for word in words {
                phrase.push(' ');
                phrase.push_str(word);
            }

            phrase
        };

        Mnemonic {
            phrase,
            lang,
            entropy
        }
    }

    /// Create a [`Mnemonic`][Mnemonic] from an existing mnemonic phrase
    ///
    /// The phrase supplied will be checked for word length and validated according to the checksum
    /// specified in BIP0039
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, Language};
    ///
    /// let phrase = "park remain person kitchen mule spell knee armed position rail grid ankle";
    /// let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();
    ///
    /// assert_eq!(phrase, mnemonic.phrase());
    /// ```
    ///
    /// [Mnemonic]: ../mnemonic/struct.Mnemonic.html
    pub fn from_phrase<S>(phrase: S, lang: Language) -> Result<Mnemonic>
    where
        S: Into<String>,
    {
        let phrase = phrase.into();

        // this also validates the checksum and phrase length before returning the entropy so we
        // can store it. We don't use the validate function here to avoid having a public API that
        // takes a phrase string and returns the entropy directly. See the Mnemonic::entropy()
        // docs for the reason.
        let entropy = Mnemonic::phrase_to_entropy(&phrase, lang)?;

        let mnemonic = Mnemonic {
            phrase,
            lang,
            entropy,
        };

        Ok(mnemonic)
    }

    /// Validate a mnemonic phrase
    ///
    /// The phrase supplied will be checked for word length and validated according to the checksum
    /// specified in BIP0039
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, Language};
    ///
    /// let test_mnemonic = "park remain person kitchen mule spell knee armed position rail grid ankle";
    ///
    /// assert!(Mnemonic::validate(test_mnemonic, Language::English).is_ok());
    /// ```
    ///
    /// [Mnemonic::from_phrase()]: ../mnemonic/struct.Mnemonic.html#method.from_phrase
    pub fn validate(phrase: &str, lang: Language) -> Result<()> {
        Mnemonic::phrase_to_entropy(phrase, lang)?;

        Ok(())
    }

    /// Calculate the checksum, verify it and return the entropy
    ///
    /// Only intended for internal use, as returning a `Vec<u8>` that looks a bit like it could be
    /// used as the seed is likely to cause problems for someone eventually. All the other functions
    /// that return something like that are explicit about what it is and what to use it for.
    fn phrase_to_entropy(phrase: &str, lang: Language) -> Result<Vec<u8>> {
        let wordmap = lang.wordmap();

        // Preallocate enough space for the longest possible word list
        let mut to_validate = BitWriter::with_capacity(264, Bits11);
        let mut word_count = 0;

        for word in phrase.split(" ") {
            let n = match wordmap.get(&word) {
                Some(&n) => n,
                None => bail!(ErrorKind::InvalidWord)
            };

            to_validate.push(n);
            word_count += 1;
        }

        let mtype = MnemonicType::for_word_count(word_count)?;

        debug_assert!(to_validate.len() == mtype.total_bits(), "Insufficient amount of bits to validate");

        let to_validate = to_validate.into_bytes();
        let entropy_bytes = mtype.entropy_bits() / 8;

        let actual_checksum = checksum(to_validate[entropy_bytes], mtype.checksum_bits());
        let entropy = truncate(to_validate, entropy_bytes);
        let checksum_byte = sha256(&entropy).as_ref()[0];
        let expected_checksum = checksum(checksum_byte, mtype.checksum_bits());

        if actual_checksum != expected_checksum {
            bail!(ErrorKind::InvalidChecksum);
        }

        Ok(entropy)
    }

    /// Get the mnemonic phrase as a string reference.
    pub fn phrase(&self) -> &str {
        &self.phrase
    }

    /// Consume the `Mnemonic` and return the phrase as a `String`.
    ///
    /// This operation doesn't perform any allocations.
    pub fn into_phrase(self) -> String {
        self.phrase
    }

    /// Get the original entropy value of the mnemonic phrase as a slice.
    ///
    /// # Example
    ///
    /// ```
    /// use bip39::{Mnemonic, Language};
    ///
    /// let phrase = "park remain person kitchen mule spell knee armed position rail grid ankle";
    ///
    /// let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();
    ///
    /// let entropy: &[u8] = mnemonic.entropy();
    /// ```
    ///
    /// **Note:** You shouldn't use the generated entropy as secrets, for that generate a new
    /// `Seed` from the `Mnemonic`.
    pub fn entropy(&self) -> &[u8] {
        &self.entropy
    }

    /// Get the [`Language`][Language]
    ///
    /// [Language]: ../language/struct.Language.html
    pub fn language(&self) -> Language {
        self.lang
    }
}

impl AsRef<str> for Mnemonic {
    fn as_ref(&self) -> &str {
        self.phrase()
    }
}

impl fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(self.phrase(), f)
    }
}

impl fmt::Debug for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.phrase(), f)
    }
}

impl fmt::LowerHex for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            f.write_str("0x")?;
        }

        for byte in self.entropy() {
            write!(f, "{:x}", byte)?;
        }

        Ok(())
    }
}

impl fmt::UpperHex for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            f.write_str("0x")?;
        }

        for byte in self.entropy() {
            write!(f, "{:X}", byte)?;
        }

        Ok(())
    }
}

impl From<Mnemonic> for String {
    fn from(val: Mnemonic) -> String {
        val.into_phrase()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn back_to_back() {
        let m1 = Mnemonic::new(MnemonicType::Words12, Language::English);
        let m2 = Mnemonic::from_phrase(m1.phrase(), Language::English).unwrap();
        let m3 = Mnemonic::from_entropy(m1.entropy(), Language::English).unwrap();

        assert_eq!(m1.entropy(), m2.entropy(), "Entropy must be the same");
        assert_eq!(m1.entropy(), m3.entropy(), "Entropy must be the same");
        assert_eq!(m1.phrase(), m2.phrase(), "Phrase must be the same");
        assert_eq!(m1.phrase(), m3.phrase(), "Phrase must be the same");
    }

    #[test]
    fn mnemonic_from_entropy() {
        let entropy = &[0x33, 0xE4, 0x6B, 0xB1, 0x3A, 0x74, 0x6E, 0xA4, 0x1C, 0xDD, 0xE4, 0x5C, 0x90, 0x84, 0x6A, 0x79];
        let phrase = "crop cash unable insane eight faith inflict route frame loud box vibrant";

        let mnemonic = Mnemonic::from_entropy(entropy, Language::English).unwrap();

        assert_eq!(phrase, mnemonic.phrase());
    }

    #[test]
    fn mnemonic_from_phrase() {
        let entropy = &[0x33, 0xE4, 0x6B, 0xB1, 0x3A, 0x74, 0x6E, 0xA4, 0x1C, 0xDD, 0xE4, 0x5C, 0x90, 0x84, 0x6A, 0x79];
        let phrase = "crop cash unable insane eight faith inflict route frame loud box vibrant";

        let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();

        assert_eq!(entropy, mnemonic.entropy());
    }

    #[test]
    fn mnemonic_format() {
        let mnemonic = Mnemonic::new(MnemonicType::Words15, Language::English);

        assert_eq!(mnemonic.phrase(), format!("{}", mnemonic));
    }

    #[test]
    fn mnemonic_hex_format() {
        let entropy = &[0x33, 0xE4, 0x6B, 0xB1, 0x3A, 0x74, 0x6E, 0xA4, 0x1C, 0xDD, 0xE4, 0x5C, 0x90, 0x84, 0x6A, 0x79];

        let mnemonic = Mnemonic::from_entropy(entropy, Language::English).unwrap();

        assert_eq!(format!("{:x}", mnemonic), "33e46bb13a746ea41cdde45c90846a79");
        assert_eq!(format!("{:#x}", mnemonic), "0x33e46bb13a746ea41cdde45c90846a79");
        assert_eq!(format!("{:X}", mnemonic), "33E46BB13A746EA41CDDE45C90846A79");
        assert_eq!(format!("{:#X}", mnemonic), "0x33E46BB13A746EA41CDDE45C90846A79");
    }
}
