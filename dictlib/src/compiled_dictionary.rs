use bit_set::BitSet;

struct CompiledDictionary
{
    character_store : CharacterStore,
    jyutping_store : JyutpingStore,
}

impl CompiledDictionary {
    fn get_jyutping_matches(&self, s : &str) -> BitSet
    {
        let mut matches = BitSet::new();

        for jyutping in &self.jyutping_store.jyutpings {
        }

        todo!()
    }
}

struct CharacterStore
{
    characters : Vec<char>,
    to_jyutping : Vec<u16>,
}

struct JyutpingStore
{
    base_strings : Vec<String>,

    jyutpings : Vec<Jyutping>,
    to_character : Vec<u16>,
}

#[derive(Debug, Clone, Copy)]
struct Jyutping
{
    // TODO merge to single u16
    base : u16,
    tone : u8,
}

