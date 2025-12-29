use std::collections::HashMap;
use std::sync::Arc;

use facet::Facet;
use facet_json_legacy as json;

type CorrMap = Option<Arc<HashMap<(String, String), f64>>>;

#[derive(Facet, Clone, Debug, Default)]
struct WordConfig {
    #[facet(default, proxy = CorrProxy)]
    corr: CorrMap,
}

#[derive(Facet, Clone, Debug, Default)]
struct TransparentWordConfig {
    #[facet(default, proxy = TransparentCorrProxy)]
    corr: CorrMap,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct CorrProxy(Vec<(String, String, f64)>);

#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(transparent)]
struct TransparentCorrProxy(Vec<(String, String, f64)>);

impl TryFrom<CorrProxy> for CorrMap {
    type Error = String;

    fn try_from(value: CorrProxy) -> Result<Self, Self::Error> {
        if value.0.is_empty() {
            return Ok(None);
        }

        let mut map = HashMap::new();
        for (a, b, weight) in value.0 {
            map.insert((a, b), weight);
        }

        Ok(Some(Arc::new(map)))
    }
}

impl TryFrom<TransparentCorrProxy> for CorrMap {
    type Error = String;

    fn try_from(value: TransparentCorrProxy) -> Result<Self, Self::Error> {
        CorrProxy(value.0).try_into()
    }
}

impl TryFrom<&CorrMap> for CorrProxy {
    type Error = String;

    fn try_from(value: &CorrMap) -> Result<Self, Self::Error> {
        let mut pairs = Vec::new();
        if let Some(map) = value {
            pairs.extend(
                map.iter()
                    .map(|((a, b), weight)| (a.clone(), b.clone(), *weight)),
            );
        }

        Ok(CorrProxy(pairs))
    }
}

impl TryFrom<&CorrMap> for TransparentCorrProxy {
    type Error = String;

    fn try_from(value: &CorrMap) -> Result<Self, Self::Error> {
        CorrProxy::try_from(value).map(|proxy| TransparentCorrProxy(proxy.0))
    }
}

#[facet_testhelpers::test]
fn parses_tuple_struct_proxy_for_map() {
    // Non-transparent tuple structs wrap their inner field inside an array.
    let json = r#"{"corr":[[["a","b",0.95],["c","d",0.42]]]}"#;
    let cfg: WordConfig = json::from_str(json).unwrap();

    let corr = cfg
        .corr
        .expect("corr should be Some after proxy conversion");
    assert_eq!(corr.get(&("a".into(), "b".into())), Some(&0.95));
    assert_eq!(corr.get(&("c".into(), "d".into())), Some(&0.42));
}

#[facet_testhelpers::test]
fn parses_transparent_proxy_for_map() {
    let json = r#"{"corr":[["a","b",0.95],["c","d",0.42]]}"#;
    let cfg: TransparentWordConfig = json::from_str(json).unwrap();

    let corr = cfg
        .corr
        .expect("corr should be Some after proxy conversion");
    assert_eq!(corr.get(&("a".into(), "b".into())), Some(&0.95));
    assert_eq!(corr.get(&("c".into(), "d".into())), Some(&0.42));
}
