use picante::{
    DynIngredient, HasRuntime, IngredientLookup, IngredientRegistry, PicanteResult, Runtime,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

static LEN_CALLS: AtomicUsize = AtomicUsize::new(0);
static SUM_CALLS: AtomicUsize = AtomicUsize::new(0);
static UNIT_CALLS: AtomicUsize = AtomicUsize::new(0);

#[picante::input]
pub struct Text {
    #[key]
    pub key: String,
    pub value: String,
}

#[picante::tracked]
pub async fn len<DB: HasTextIngredient>(db: &DB, text: Text) -> PicanteResult<u64> {
    LEN_CALLS.fetch_add(1, Ordering::Relaxed);
    Ok(text.value(db)?.len() as u64)
}

#[picante::tracked]
pub async fn sum<DB>(db: &DB, x: u32, y: u32) -> u64 {
    let _ = db;
    SUM_CALLS.fetch_add(1, Ordering::Relaxed);
    (x as u64) + (y as u64)
}

#[picante::tracked]
pub async fn unit_key<DB>(db: &DB) -> u64 {
    let _ = db;
    UNIT_CALLS.fetch_add(1, Ordering::Relaxed);
    42
}

#[picante::interned]
pub struct Word {
    pub text: String,
}

struct Db {
    runtime: Runtime,
    ingredients: IngredientRegistry<Db>,
    text_keys: Arc<TextKeysIngredient>,
    text_data: Arc<TextDataIngredient>,
    len: Arc<LenQuery<Db>>,
    sum: Arc<SumQuery<Db>>,
    unit_key: Arc<UnitKeyQuery<Db>>,
    word: Arc<WordIngredient>,
}

impl Db {
    fn new() -> Self {
        let runtime = Runtime::new();
        let mut ingredients = IngredientRegistry::new();

        let text_keys = make_text_keys();
        let text_data = make_text_data();
        let len = make_len_query::<Db>();
        let sum = make_sum_query::<Db>();
        let unit_key = make_unit_key_query::<Db>();
        let word = make_word_ingredient();

        ingredients.register(text_keys.clone());
        ingredients.register(text_data.clone());
        ingredients.register(len.clone());
        ingredients.register(sum.clone());
        ingredients.register(unit_key.clone());
        ingredients.register(word.clone());

        Self {
            runtime,
            ingredients,
            text_keys,
            text_data,
            len,
            sum,
            unit_key,
            word,
        }
    }
}

impl HasRuntime for Db {
    fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl IngredientLookup for Db {
    fn ingredient(&self, kind: picante::QueryKindId) -> Option<&dyn DynIngredient<Self>> {
        self.ingredients.ingredient(kind)
    }
}

impl HasTextIngredient for Db {
    fn text_keys(&self) -> &TextKeysIngredient {
        &self.text_keys
    }

    fn text_data(&self) -> &TextDataIngredient {
        &self.text_data
    }
}

impl HasLenQuery for Db {
    fn len_query(&self) -> &LenQuery<Self> {
        &self.len
    }
}

impl HasSumQuery for Db {
    fn sum_query(&self) -> &SumQuery<Self> {
        &self.sum
    }
}

impl HasUnitKeyQuery for Db {
    fn unit_key_query(&self) -> &UnitKeyQuery<Self> {
        &self.unit_key
    }
}

impl HasWordIngredient for Db {
    fn word_ingredient(&self) -> &WordIngredient {
        &self.word
    }
}

#[tokio::test(flavor = "current_thread")]
async fn macros_basic_flow() -> PicanteResult<()> {
    LEN_CALLS.store(0, Ordering::Relaxed);
    let db = Db::new();
    let text = Text::new(&db, "a".into(), "hello".into())?;

    assert_eq!(len(&db, text).await?, 5);
    assert_eq!(len(&db, text).await?, 5);
    assert_eq!(LEN_CALLS.load(Ordering::Relaxed), 1);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn macros_tuple_keys_and_unit_key() -> PicanteResult<()> {
    SUM_CALLS.store(0, Ordering::Relaxed);
    UNIT_CALLS.store(0, Ordering::Relaxed);

    let db = Db::new();

    assert_eq!(sum(&db, 1, 2).await?, 3);
    assert_eq!(sum(&db, 1, 2).await?, 3);
    assert_eq!(SUM_CALLS.load(Ordering::Relaxed), 1);

    assert_eq!(sum(&db, 2, 3).await?, 5);
    assert_eq!(sum(&db, 2, 3).await?, 5);
    assert_eq!(SUM_CALLS.load(Ordering::Relaxed), 2);

    assert_eq!(unit_key(&db).await?, 42);
    assert_eq!(unit_key(&db).await?, 42);
    assert_eq!(UNIT_CALLS.load(Ordering::Relaxed), 1);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn macros_interned_works() -> PicanteResult<()> {
    let db = Db::new();

    let w1 = Word::new(&db, "hello".to_string())?;
    let w2 = Word::new(&db, "hello".to_string())?;
    assert_eq!(w1, w2);
    assert_eq!(w1.text(&db)?, "hello".to_string());

    Ok(())
}
