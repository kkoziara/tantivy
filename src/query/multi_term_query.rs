use schema::Term;
use query::Query;
use common::TimerTree;
use common::OpenTimer;
use std::io;
use core::searcher::Searcher;
use collector::Collector;
use SegmentLocalId;
use core::SegmentReader;
use postings::SegmentPostings;
use postings::UnionPostings;
use postings::ScoredDocSet;
use postings::DocSet;
use query::MultiTermScorer;
use std::iter;
use fastfield::U32FastFieldReader;
use ScoredDoc;


pub struct MultiTermQuery {
    terms: Vec<Term>,    
}

impl Query for MultiTermQuery {

    fn search<C: Collector>(&self, searcher: &Searcher, collector: &mut C) -> io::Result<TimerTree> {
        let mut timer_tree = TimerTree::new();
        
        let multi_term_scorer = self.scorer(searcher);
        {
            let mut search_timer = timer_tree.open("search");
            for (segment_ord, segment_reader) in searcher.segments().iter().enumerate() {
                let mut segment_search_timer = search_timer.open("segment_search");
                {
                    let _ = segment_search_timer.open("set_segment");
                    try!(collector.set_segment(segment_ord as SegmentLocalId, &segment_reader));
                }
                let mut postings = self.search_segment(
                        segment_reader,
                        multi_term_scorer.clone(),
                        segment_search_timer.open("get_postings"));
                {
                    let _collection_timer = segment_search_timer.open("collection");
                    while postings.next() {
                        let scored_doc = ScoredDoc(postings.score(), postings.doc());
                        collector.collect(scored_doc);
                    }
                }
            }
        }
        Ok(timer_tree)
    }
}

impl MultiTermQuery {
    
    fn scorer(&self, searcher: &Searcher) -> MultiTermScorer {
        let idfs: Vec<f32> = self.terms.iter()
            .map(|term| searcher.doc_freq(term))
            .map(|doc_freq| {
                if doc_freq == 0 {
                    return 1.
                }
                else {
                    1.0 / (doc_freq as f32)
                }
            })
            .collect();
        let query_coord = iter::repeat(1f32).take(self.terms.len()).collect();
        MultiTermScorer::new(query_coord, idfs)
    }

    pub fn new(terms: Vec<Term>) -> MultiTermQuery {
        MultiTermQuery {
            terms: terms,
        }
    }
        
    fn search_segment<'a, 'b>(&'b self, reader: &'b SegmentReader, multi_term_scorer: MultiTermScorer, mut timer: OpenTimer<'a>) -> UnionPostings<SegmentPostings> {
        let mut segment_postings: Vec<SegmentPostings> = Vec::with_capacity(self.terms.len());
        let mut fieldnorms_readers: Vec<U32FastFieldReader> = Vec::with_capacity(self.terms.len());
        {
            let mut decode_timer = timer.open("decode_all");
            for term in &self.terms {
                let _decode_one_timer = decode_timer.open("decode_one");
                match reader.read_postings(term) {
                    Some(postings) => {
                        let field = term.get_field();
                        fieldnorms_readers.push(reader.get_fieldnorms_reader(field).unwrap());
                        segment_postings.push(postings);
                    }
                    None => {
                        segment_postings.push(SegmentPostings::empty());
                    }
                }
            }
        }
        UnionPostings::new(fieldnorms_readers, segment_postings, multi_term_scorer)
    }
}