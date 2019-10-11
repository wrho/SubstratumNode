// Copyright (c) 2017-2019, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.
use crate::blockchain::blockchain_interface::contract_address;
use crate::sub_lib::cryptde;
use crate::sub_lib::cryptde::CryptDE;
use crate::sub_lib::cryptde::CryptData;
use crate::sub_lib::cryptde::CryptdecError;
use crate::sub_lib::cryptde::PlainData;
use crate::sub_lib::cryptde::PrivateKey;
use crate::sub_lib::cryptde::PublicKey;
use rand::prelude::*;

#[derive(Debug, Clone)]
pub struct CryptDENull {
    private_key: PrivateKey,
    public_key: PublicKey,
    digest: [u8; 32],
}

impl CryptDE for CryptDENull {
    fn encode(&self, public_key: &PublicKey, data: &PlainData) -> Result<CryptData, CryptdecError> {
        Self::encode_with_key_data(public_key.as_slice(), data)
    }

    fn decode(&self, data: &CryptData) -> Result<PlainData, CryptdecError> {
        Self::decode_with_key_data(self.private_key.as_slice(), data)
    }

    fn random(&self, dest: &mut [u8]) {
        for i in 0..dest.len() {
            dest[i] = '4' as u8
        }
    }

    fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }

    fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    // This is dup instead of clone because it returns a Box<CryptDE> instead of a CryptDENull.
    fn dup(&self) -> Box<dyn CryptDE> {
        Box::new(CryptDENull {
            private_key: self.private_key.clone(),
            public_key: self.public_key.clone(),
            digest: self.digest,
        })
    }

    fn sign(&self, data: &PlainData) -> Result<CryptData, CryptdecError> {
        let hash = self.hash(data);
        Self::encode_with_key_data(
            self.private_key().as_slice(),
            &PlainData::new(hash.as_slice()),
        )
    }

    fn verify_signature(
        &self,
        data: &PlainData,
        signature: &CryptData,
        public_key: &PublicKey,
    ) -> bool {
        let claimed_hash = match Self::decode_with_key_data(public_key.as_slice(), signature) {
            Err(_) => return false,
            Ok(hash) => CryptData::new(hash.as_slice()),
        };
        let actual_hash = self.hash(data);
        actual_hash == claimed_hash
    }

    fn hash(&self, data: &PlainData) -> CryptData {
        let mut hash = sha1::Sha1::new();
        hash.update(data.as_slice());
        CryptData::new(&hash.digest().bytes())
    }

    fn public_key_to_descriptor_fragment(&self, public_key: &PublicKey) -> String {
        base64::encode_config(public_key.as_slice(), base64::STANDARD_NO_PAD)
    }

    fn descriptor_fragment_to_first_contact_public_key(
        &self,
        descriptor_fragment: &str,
    ) -> Result<PublicKey, String> {
        if descriptor_fragment.is_empty() {
            return Err("Public key cannot be empty".to_string());
        }
        let half_key = match base64::decode_config(descriptor_fragment, base64::STANDARD_NO_PAD) {
            Ok(half_key) => half_key,
            Err(_) => {
                return Err(format!(
                    "Invalid Base64 value for public key: {}",
                    descriptor_fragment
                ))
            }
        };
        Ok(PublicKey::from(half_key))
    }

    fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

impl CryptDENull {
    pub fn new(chain_id: u8) -> Self {
        let mut private_key = [0; 32];
        let mut rng = thread_rng();
        for idx in 0..32 {
            private_key[idx] = rng.gen();
        }
        let private_key = PrivateKey::from(&private_key[..]);
        let public_key = Self::public_from_private(&private_key);
        let digest = cryptde::create_digest(&public_key, &contract_address(chain_id));
        Self {
            private_key,
            public_key,
            digest,
        }
    }
    pub fn from(public_key: &PublicKey, chain_id: u8) -> CryptDENull {
        let mut result = CryptDENull::new(chain_id);
        result.set_key_pair(public_key, chain_id);
        result
    }

    pub fn set_key_pair(&mut self, public_key: &PublicKey, chain_id: u8) {
        self.public_key = public_key.clone();
        self.private_key = CryptDENull::private_from_public(public_key);
        self.digest = cryptde::create_digest(public_key, &contract_address(chain_id));
    }

    pub fn private_from_public(in_key: &PublicKey) -> PrivateKey {
        PrivateKey::new(&Self::other_key_data(in_key.as_slice()))
    }

    pub fn public_from_private(in_key: &PrivateKey) -> PublicKey {
        PublicKey::new(&Self::other_key_data(in_key.as_slice()))
    }

    pub fn other_key_data(in_key_data: &[u8]) -> Vec<u8> {
        in_key_data.iter().map(|b| (*b).wrapping_add(128)).collect()
    }

    fn encode_with_key_data(key_data: &[u8], data: &PlainData) -> Result<CryptData, CryptdecError> {
        if key_data.is_empty() {
            Err(CryptdecError::EmptyKey)
        } else if data.is_empty() {
            Err(CryptdecError::EmptyData)
        } else {
            let other_key = Self::other_key_data(key_data);
            Ok(CryptData::new(
                &[&other_key.as_slice(), data.as_slice()].concat()[..],
            ))
        }
    }

    fn decode_with_key_data(key_data: &[u8], data: &CryptData) -> Result<PlainData, CryptdecError> {
        if key_data.is_empty() {
            Err(CryptdecError::EmptyKey)
        } else if data.is_empty() {
            Err(CryptdecError::EmptyData)
        } else if key_data.len() > data.len() {
            Err(CryptdecError::InvalidKey(CryptDENull::invalid_key_message(
                key_data, data,
            )))
        } else {
            let (k, d) = data.as_slice().split_at(key_data.len());
            if k != key_data {
                Err(CryptdecError::InvalidKey(CryptDENull::invalid_key_message(
                    key_data, data,
                )))
            } else {
                Ok(PlainData::new(d))
            }
        }
    }

    fn invalid_key_message(key_data: &[u8], data: &CryptData) -> String {
        let prefix_len = std::cmp::min(key_data.len(), data.len());
        let vec = Vec::from(&data.as_slice()[0..prefix_len]);
        format!(
            "Could not decrypt with {:?} data beginning with {:?}",
            key_data, vec
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{cryptde, DEFAULT_CHAIN_ID};
    use ethsign_crypto::Keccak256;

    #[test]
    fn encode_with_empty_key() {
        let subject = cryptde();

        let result = subject.encode(&PublicKey::new(b""), &PlainData::new(b"data"));

        assert_eq!(CryptdecError::EmptyKey, result.err().unwrap());
    }

    #[test]
    fn encode_with_empty_data() {
        let subject = cryptde();

        let result = subject.encode(&PublicKey::new(b"key"), &PlainData::new(b""));

        assert_eq!(CryptdecError::EmptyData, result.err().unwrap());
    }

    #[test]
    fn encode_with_key_and_data() {
        let subject = cryptde();

        let result = subject.encode(&PublicKey::new(b"key"), &PlainData::new(b"data"));

        let mut data: Vec<u8> = CryptDENull::private_from_public(&PublicKey::new(b"key")).into();
        data.extend(b"data".iter());
        assert_eq!(CryptData::new(&data[..]), result.ok().unwrap());
    }

    #[test]
    fn decode_with_empty_key() {
        let mut subject = cryptde().clone();
        subject.private_key = PrivateKey::new(b"");

        let result = subject.decode(&CryptData::new(b"keydata"));

        assert_eq!(CryptdecError::EmptyKey, result.err().unwrap());
    }

    #[test]
    fn decode_with_empty_data() {
        let mut subject = cryptde().clone();
        subject.private_key = PrivateKey::new(b"key");

        let result = subject.decode(&CryptData::new(b""));

        assert_eq!(CryptdecError::EmptyData, result.err().unwrap());
    }

    #[test]
    fn decode_with_key_and_data() {
        let mut subject = cryptde().clone();
        subject.private_key = PrivateKey::new(b"key");

        let result = subject.decode(&CryptData::new(b"keydata"));

        assert_eq!(PlainData::new(b"data"), result.ok().unwrap());
    }

    #[test]
    fn decode_with_invalid_key_and_data() {
        let mut subject = cryptde().clone();
        subject.private_key = PrivateKey::new(b"badKey");

        let result = subject.decode(&CryptData::new(b"keydataxyz"));

        assert_eq!(CryptdecError::InvalidKey (String::from ("Could not decrypt with [98, 97, 100, 75, 101, 121] data beginning with [107, 101, 121, 100, 97, 116]")), result.err().unwrap());
    }

    #[test]
    fn decode_with_key_exceeding_data_length() {
        let mut subject = cryptde().clone();
        subject.private_key = PrivateKey::new(b"invalidkey");

        let result = subject.decode(&CryptData::new(b"keydata"));

        assert_eq!(CryptdecError::InvalidKey (String::from ("Could not decrypt with [105, 110, 118, 97, 108, 105, 100, 107, 101, 121] data beginning with [107, 101, 121, 100, 97, 116, 97]")), result.err().unwrap());
    }

    #[test]
    fn random_is_pretty_predictable() {
        let subject = cryptde();
        let mut dest: [u8; 11] = [0; 11];

        subject.random(&mut dest[..]);

        assert_eq!(&b"44444444444"[..], dest);
    }

    #[test]
    fn construction_produces_different_keys_each_time() {
        let subject1 = CryptDENull::new(DEFAULT_CHAIN_ID);
        let subject2 = CryptDENull::new(DEFAULT_CHAIN_ID);

        let first_public = subject1.public_key().clone();
        let first_private = subject1.private_key().clone();

        let second_public = subject2.public_key().clone();
        let second_private = subject2.private_key().clone();

        assert_ne!(second_public, first_public);
        assert_ne!(second_private, first_private);
    }

    #[test]
    fn generated_keys_work_with_each_other() {
        let subject = cryptde();

        let expected_data = PlainData::new(&b"These are the times that try men's souls"[..]);
        let encrypted_data = subject
            .encode(&subject.public_key(), &expected_data)
            .unwrap();
        let decrypted_data = subject.decode(&encrypted_data).unwrap();
        assert_eq!(expected_data, decrypted_data);
    }

    #[test]
    fn private_and_public_keys_are_different_and_derivable_from_each_other() {
        let original_private_key = PrivateKey::new(b"The quick brown fox jumps over the lazy dog");

        let public_key = CryptDENull::public_from_private(&original_private_key);
        let resulting_private_key = CryptDENull::private_from_public(&public_key);

        assert_ne!(original_private_key.as_slice(), public_key.as_slice());
        assert_eq!(original_private_key, resulting_private_key);
    }

    #[test]
    fn from_and_setting_key_pair_works() {
        let public_key = PublicKey::new(b"The quick brown fox jumps over the lazy dog");

        let subject = CryptDENull::from(&public_key, DEFAULT_CHAIN_ID);

        let expected_data = PlainData::new(&b"These are the times that try men's souls"[..]);
        let encrypted_data = subject.encode(&public_key, &expected_data).unwrap();
        let decrypted_data = subject.decode(&encrypted_data).unwrap();
        assert_eq!(expected_data, decrypted_data);
        let encrypted_data = subject
            .encode(&subject.public_key(), &expected_data)
            .unwrap();
        let decrypted_data = subject.decode(&encrypted_data).unwrap();
        assert_eq!(expected_data, decrypted_data);
    }

    #[test]
    fn dup_works() {
        let subject = cryptde();

        let result = subject.dup();

        assert_eq!(result.public_key(), subject.public_key());
        assert_eq!(result.private_key(), subject.private_key());
    }

    #[test]
    fn stringifies_public_key_properly() {
        let subject = cryptde();
        let public_key = PublicKey::new(&[1, 2, 3, 4]);

        let result = subject.public_key_to_descriptor_fragment(&public_key);

        assert_eq!(result, "AQIDBA".to_string());
    }

    #[test]
    fn destringifies_public_key_properly() {
        let subject = cryptde();
        let half_key = "AQIDBA";

        let result = subject.descriptor_fragment_to_first_contact_public_key(half_key);

        assert_eq!(result, Ok(PublicKey::new(&[1, 2, 3, 4])));
    }

    #[test]
    fn fails_to_destringify_public_key_properly() {
        let subject = cryptde();
        let half_key = "((]--$";

        let result = subject.descriptor_fragment_to_first_contact_public_key(half_key);

        assert_eq!(
            result,
            Err(String::from("Invalid Base64 value for public key: ((]--$"))
        );
    }

    const HASHABLE_DATA: &str = "Availing himself of the mild, summer-cool weather that now reigned \
        in these latitudes, and in preparation for the peculiarly active pursuits shortly to be \
        anticipated, Perth, the begrimed, blistered old blacksmith, had not removed his portable \
        forge to the hold again, after concluding his contributory work for Ahab's leg, but still \
        retained it on deck, fast lashed to ringbolts by the foremast; being now almost incessantly \
        invoked by the headsmen, and harpooneers, and bowsmen to do some little job for them; \
        altering, or repairing, or new shaping their various weapons and boat furniture. Often \
        he would be surrounded by an eager circle, all waiting to be served; holding boat-spades, \
        pike-heads, harpoons, and lances, and jealously watching his every sooty movement, as he \
        toiled. Nevertheless, this old man's was a patient hammer wielded by a patient arm. No \
        murmur, no impatience, no petulance did come from him. Silent, slow, and solemn; bowing \
        over still further his chronically broken back, he toiled away, as if toil were life \
        itself, and the heavy beating of his hammer the heavy beating of his heart. And so it \
        was.—Most miserable! A peculiar walk in this old man, a certain slight but painful \
        appearing yawing in his gait, had at an early period of the voyage excited the curiosity \
        of the mariners. And to the importunity of their persisted questionings he had finally \
        given in; and so it came to pass that every one now knew the shameful story of his wretched \
        fate. Belated, and not innocently, one bitter winter's midnight, on the road running \
        between two country towns, the blacksmith half-stupidly felt the deadly numbness stealing \
        over him, and sought refuge in a leaning, dilapidated barn. The issue was, the loss of the \
        extremities of both feet. Out of this revelation, part by part, at last came out the four \
        acts of the gladness, and the one long, and as yet uncatastrophied fifth act of the grief \
        of his life's drama. He was an old man, who, at the age of nearly sixty, had postponedly \
        encountered that thing in sorrow's technicals called ruin. He had been an artisan of famed \
        excellence, and with plenty to do; owned a house and garden; embraced a youthful, \
        daughter-like, loving wife, and three blithe, ruddy children; every Sunday went to a \
        cheerful-looking church, planted in a grove. But one night, under cover of darkness, and \
        further concealed in a most cunning disguisement, a desperate burglar slid into his happy \
        home, and robbed them all of everything. And darker yet to tell, the blacksmith himself \
        did ignorantly conduct this burglar into his family's heart. It was the Bottle Conjuror! \
        Upon the opening of that fatal cork, forth flew the fiend, and shrivelled up his home. \
        Now, for prudent, most wise, and economic reasons, the blacksmith's shop was in the \
        basement of his dwelling, but with a separate entrance to it; so that always had the \
        young and loving healthy wife listened with no unhappy nervousness, but with vigorous \
        pleasure, to the stout ringing of her young-armed old husband's hammer; whose \
        reverberations, muffled by passing through the floors and walls, came up to her, not \
        unsweetly, in her nursery; and so, to stout Labor's iron lullaby, the blacksmith's \
        infants were rocked to slumber. Oh, woe on woe! Oh, Death, why canst thou not sometimes \
        be timely? Hadst thou taken this old blacksmith to thyself ere his full ruin came upon \
        him, then had the young widow had a delicious grief, and her orphans a truly venerable, \
        legendary sire to dream of in their after years; and all of them a care-killing competency.";

    #[test]
    fn verifying_a_good_signature_works() {
        let data = PlainData::new(HASHABLE_DATA.as_bytes());
        let subject = cryptde();

        let signature = subject.sign(&data).unwrap();
        let result = subject.verify_signature(&data, &signature, &subject.public_key());

        assert_eq!(true, result);
    }

    #[test]
    fn verifying_a_bad_signature_fails() {
        let data = PlainData::new(HASHABLE_DATA.as_bytes());
        let subject = cryptde();
        let mut modified = Vec::from(HASHABLE_DATA.as_bytes());
        modified[0] = modified[0] + 1;
        let different_data = PlainData::from(modified);
        let signature = subject.sign(&data).unwrap();

        let result = subject.verify_signature(&different_data, &signature, &subject.public_key());

        assert_eq!(false, result);
    }

    #[test]
    fn hashing_produces_the_same_value_for_the_same_data() {
        let some_data = PlainData::new(HASHABLE_DATA.as_bytes());
        let more_data = some_data.clone();
        let subject = cryptde();

        let some_result = subject.hash(&some_data);
        let more_result = subject.hash(&more_data);

        assert_eq!(some_result, more_result);
    }

    #[test]
    fn hashing_produces_different_values_for_different_data() {
        let some_data = PlainData::new(HASHABLE_DATA.as_bytes());
        let mut modified = Vec::from(HASHABLE_DATA.as_bytes());
        modified[0] = modified[0] + 1;
        let different_data = PlainData::from(modified);
        let subject = cryptde();

        let some_result = subject.hash(&some_data);
        let different_result = subject.hash(&different_data);

        assert_ne!(some_result, different_result);
    }

    #[test]
    fn hashing_produces_the_same_length_for_long_and_short_data() {
        let long_data = PlainData::new(HASHABLE_DATA.as_bytes());
        let short_data = PlainData::new(&[1, 2, 3, 4]);
        let subject = cryptde();

        let long_result = subject.hash(&long_data);
        let short_result = subject.hash(&short_data);

        assert_eq!(long_result.len(), short_result.len());
    }

    #[test]
    fn hashing_produces_a_digest_with_the_smart_contract_address() {
        let subject = &cryptde();
        let merged = [
            subject.public_key().as_ref(),
            contract_address(DEFAULT_CHAIN_ID).as_ref(),
        ]
        .concat();
        let expected_digest = merged.keccak256();

        let actual_digest = subject.digest();

        assert_eq!(expected_digest, actual_digest);
    }

    #[test]
    fn creating_cryptde_produces_the_same_digest() {
        let subject_one = &CryptDENull::new(DEFAULT_CHAIN_ID);
        let subject_two = &CryptDENull::from(subject_one.public_key(), DEFAULT_CHAIN_ID);
        let subject_three = &mut CryptDENull::new(DEFAULT_CHAIN_ID);
        subject_three.set_key_pair(subject_two.public_key(), DEFAULT_CHAIN_ID);

        assert_eq!(subject_one.digest(), subject_two.digest());
        assert_eq!(subject_one.digest(), subject_three.digest());
    }
}
