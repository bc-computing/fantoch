use crate::bote::protocol::Protocol;
use crate::bote::stats::Stats;
use crate::bote::Bote;
use crate::planet::{Planet, Region};
use permutator::Combination;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::iter::FromIterator;

// mapping from protocol name to its stats
type AllStats = BTreeMap<String, Stats>;
// config score and stats (more like: score, config and stats)
type ConfigSS = (isize, BTreeSet<Region>, AllStats);

struct SearchParams {
    min_lat_improv: isize,
    min_fairness_improv: isize,
    max_n: usize,
    search_metric: SearchMetric,
    search_ft_filter: SearchFTFilter,
    clients: Vec<Region>,
    regions: Vec<Region>,
}

impl SearchParams {
    pub fn new(
        min_lat_improv: isize,
        min_fairness_improv: isize,
        max_n: usize,
        search_metric: SearchMetric,
        search_ft_filter: SearchFTFilter,
        clients: Vec<Region>,
        regions: Vec<Region>,
    ) -> Self {
        SearchParams {
            min_lat_improv,
            min_fairness_improv,
            max_n,
            search_metric,
            search_ft_filter,
            clients,
            regions,
        }
    }
}

pub struct Search {
    params: SearchParams,
    bote: Bote,
    all_configs: HashMap<usize, BTreeSet<ConfigSS>>,
}

impl Search {
    pub fn new(
        min_lat_improv: isize,
        min_fairness_improv: isize,
        max_n: usize,
        search_metric: SearchMetric,
        search_ft_filter: SearchFTFilter,
        search_input: SearchInput,
        lat_dir: &str,
    ) -> Self {
        // create planet
        let planet = Planet::new(lat_dir);

        // get regions for servers and clients
        let (clients, regions) = Self::search_regions(&search_input, &planet);

        // create bote
        let bote = Bote::from(planet);

        // create search params
        let params = SearchParams::new(
            min_lat_improv,
            min_fairness_improv,
            max_n,
            search_metric,
            search_ft_filter,
            clients,
            regions,
        );

        // create empty config and get all configs
        let all_configs = Self::all_configs(&params, &bote);

        // return a new `Search` instance
        Search {
            params,
            bote,
            all_configs,
        }
    }

    pub fn evolving_configs(&self) {
        // create result variable
        let mut configs = BTreeSet::new();

        self.superset_configs(3)
            .for_each(|(score3, config3, stats3)| {
                self.superset_configs(5)
                    .filter(|(_, config5, _)| config3.is_subset(config5))
                    .for_each(|(score5, config5, stats5)| {
                        self.superset_configs(7)
                            .filter(|(_, config7, _)| {
                                config5.is_subset(config7)
                            })
                            .for_each(|(score7, config7, stats7)| {
                                self.superset_configs(9)
                                    .filter(|(_, config9, _)| {
                                        config7.is_subset(config9)
                                    })
                                    .for_each(|(score9, config9, stats9)| {
                                        let score =
                                            score3 + score5 + score7 + score9;
                                        let config = vec![
                                            (config3, stats3),
                                            (config5, stats5),
                                            (config7, stats7),
                                            (config9, stats9),
                                        ];
                                        assert!(configs.insert((score, config)))
                                    });
                            });
                    });
            });

        Self::show(configs)
    }

    fn show(configs: BTreeSet<(isize, Vec<(&BTreeSet<Region>, &AllStats)>)>) {
        let max_configs = 1000;
        for (score, config_evolution) in
            configs.into_iter().rev().take(max_configs)
        {
            let mut sorted_config = Vec::new();
            print!("{}", score);
            for (config, stats) in config_evolution {
                // update sorted config
                for region in config {
                    if !sorted_config.contains(&region) {
                        sorted_config.push(region)
                    }
                }

                // compute n and max f
                let n = config.len();

                print!(" | [n={}]", n);

                // and show stats for all possible f
                for f in 1..=Self::max_f(n) {
                    let atlas =
                        stats.get(&Self::protocol_key("atlas", f)).unwrap();
                    let fpaxos =
                        stats.get(&Self::protocol_key("fpaxos", f)).unwrap();
                    print!(" a{}={:?} f{}={:?}", f, atlas, f, fpaxos);
                }
                let epaxos = stats.get(&Self::epaxos_protocol_key()).unwrap();
                print!(" e={:?}", epaxos);
            }
            print!("\n");
            println!("{:?}", sorted_config);
        }
    }

    /// find configurations such that:
    /// - their size is `n`
    /// - are a superset of `previous_config`
    fn superset_configs(&self, n: usize) -> impl Iterator<Item = &ConfigSS> {
        self.all_configs.get(&n).unwrap().into_iter()
    }

    fn all_configs(
        params: &SearchParams,
        bote: &Bote,
    ) -> HashMap<usize, BTreeSet<ConfigSS>> {
        (3..=params.max_n)
            .step_by(2)
            .map(|n| {
                let configs = params
                    .regions
                    .combination(n)
                    .filter_map(|config| {
                        // clone config
                        let config: Vec<Region> =
                            config.into_iter().cloned().collect();

                        // compute config score
                        match Self::compute_score(&config, params, bote) {
                            (true, score, stats) => Some((
                                score,
                                BTreeSet::from_iter(config.into_iter()),
                                stats,
                            )),
                            _ => None,
                        }
                    })
                    .collect();
                (n, configs)
            })
            .collect()
    }

    fn compute_score(
        config: &Vec<Region>,
        params: &SearchParams,
        bote: &Bote,
    ) -> (bool, isize, AllStats) {
        // compute n
        let n = config.len();

        // compute stats for all protocols
        let stats = Self::compute_stats(config, params, bote);

        // compute score and check if it is a valid configuration
        let mut valid = true;
        let mut score: isize = 0;
        let mut count: isize = 0;

        // f values accounted for when computing score and config validity
        let fs = params.search_ft_filter.fs(n);

        for f in fs.into_iter() {
            let atlas = stats.get(&Self::protocol_key("atlas", f)).unwrap();
            let fpaxos = stats.get(&Self::protocol_key("fpaxos", f)).unwrap();

            // compute improvements of atlas wrto to fpaxos
            let lat_improv = (fpaxos.mean() as isize) - (atlas.mean() as isize);
            let fairness_improv =
                (fpaxos.fairness() as isize) - (atlas.fairness() as isize);
            let min_max_dist_improv = (fpaxos.min_max_dist() as isize)
                - (atlas.min_max_dist() as isize);

            // compute its score depending on the search metric
            score += match params.search_metric {
                SearchMetric::Latency => lat_improv,
                SearchMetric::Fairness => fairness_improv,
                SearchMetric::MinMaxDistance => min_max_dist_improv,
                SearchMetric::LatencyAndFairness => {
                    lat_improv + fairness_improv
                }
            };
            count += 1;

            // check if this config is valid
            valid = valid
                && lat_improv >= params.min_lat_improv
                && fairness_improv >= params.min_fairness_improv;
        }

        // get score average
        score = score / count;

        (valid, score, stats)
    }

    fn compute_stats(
        config: &Vec<Region>,
        params: &SearchParams,
        bote: &Bote,
    ) -> AllStats {
        // compute n
        let n = config.len();
        let mut stats = BTreeMap::new();

        for f in 1..=Self::max_f(n) {
            // compute atlas stats
            let atlas = bote.leaderless(
                config,
                &params.clients,
                Protocol::Atlas.quorum_size(n, f),
            );
            stats.insert(Self::protocol_key("atlas", f), atlas);

            // compute fpaxos stats
            let fpaxos = bote.best_mean_leader(
                config,
                &params.clients,
                Protocol::FPaxos.quorum_size(n, f),
            );
            stats.insert(Self::protocol_key("fpaxos", f), fpaxos);
        }

        // compute epaxos stats
        let epaxos = bote.leaderless(
            config,
            &params.clients,
            Protocol::EPaxos.quorum_size(n, 0),
        );
        stats.insert(Self::epaxos_protocol_key(), epaxos);

        // return all stats
        stats
    }

    fn max_f(n: usize) -> usize {
        let max_f = 3;
        std::cmp::min(n / 2 as usize, max_f)
    }

    fn protocol_key(prefix: &str, f: usize) -> String {
        format!("{}f{}", prefix, f).to_string()
    }

    fn epaxos_protocol_key() -> String {
        "epaxos".to_string()
    }

    fn search_regions(
        search_input: &SearchInput,
        planet: &Planet,
    ) -> (Vec<Region>, Vec<Region>) {
        // compute all regions
        let mut regions = planet.regions();
        regions.sort();

        // compute clients11
        let mut clients11 = vec![
            Region::new("asia-east2"),
            Region::new("asia-northeast1"),
            Region::new("asia-south1"),
            Region::new("asia-southeast1"),
            Region::new("australia-southeast1"),
            Region::new("europe-north1"),
            Region::new("europe-west2"),
            Region::new("northamerica-northeast1"),
            Region::new("southamerica-east1"),
            Region::new("us-east1"),
            Region::new("us-west2"),
        ];
        clients11.sort();

        // compute clients9
        let mut clients9 = vec![
            Region::new("asia-east2"),
            Region::new("asia-northeast1"),
            Region::new("asia-south1"),
            Region::new("australia-southeast1"),
            Region::new("europe-north1"),
            Region::new("europe-west2"),
            Region::new("southamerica-east1"),
            Region::new("us-east1"),
            Region::new("us-west2"),
        ];
        clients9.sort();

        match search_input {
            SearchInput::C20R20 => (regions.clone(), regions),
            SearchInput::C11R20 => (clients11, regions),
            SearchInput::C11R11 => (clients11.clone(), clients11),
            SearchInput::C09R09 => (clients9.clone(), clients9),
        }
    }
}

/// identifies which regions considered for the search
#[allow(dead_code)]
pub enum SearchInput {
    /// 20-clients considered, config search within the 20 regions
    C20R20,
    /// 11-clients considered, config search within the 20 regions
    C11R20,
    /// 11-clients considered, config search within the same 11 regions
    C11R11,
    /// 9-clients considered, config search within the same 9 regions
    C09R09,
}

/// what's consider when raking configurations
#[allow(dead_code)]
pub enum SearchMetric {
    Latency,
    Fairness,
    MinMaxDistance,
    LatencyAndFairness,
}

/// fault tolerance considered when searching for configurations
#[allow(dead_code)]
pub enum SearchFTFilter {
    F1,
    F2,
    F1AndF2,
}

impl SearchFTFilter {
    fn fs(&self, n: usize) -> Vec<usize> {
        match self {
            SearchFTFilter::F1 => vec![1],
            SearchFTFilter::F2 => {
                if n == 3 {
                    vec![1]
                } else {
                    vec![2]
                }
            }
            SearchFTFilter::F1AndF2 => {
                if n == 3 {
                    vec![1]
                } else {
                    vec![1, 2]
                }
            }
        }
    }
}
