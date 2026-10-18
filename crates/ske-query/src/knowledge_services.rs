//! Snapshot-scoped, read-only knowledge services. SQL stays behind Query.
use super::*;

impl Query {
    pub fn closed_snapshot(&self, snapshot: &str, repository: Option<&str>) -> Result<String> {
        let id = self.snapshot(snapshot, repository)?;
        let state = one(self.db(), "SELECT status FROM snapshots WHERE id=?1", [id])?;
        if state["status"] != "CLOSED" {
            return Err(Error::new(
                "E_USAGE",
                "knowledge services require a CLOSED snapshot",
            ));
        }
        Ok(id.to_string())
    }

    /// Read persisted summaries only; missing analysis is not computed implicitly.
    pub fn get_function_summary(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        function: &str,
    ) -> Result<Value> {
        let sid = self.closed_snapshot(snapshot, repository)?;
        let (fid, _) = self.function(&sid, repository, function)?;
        let summaries = rows(self.db(),
            "SELECT fs.id,fs.context_fingerprint,fs.summary_fingerprint,fs.precision,fs.complete,fs.unknowns_json,fs.produced_by_run_id FROM function_summaries fs JOIN analysis_runs r ON r.id=fs.produced_by_run_id WHERE fs.function_id=?1 AND r.snapshot_id=?2 ORDER BY fs.id",
            params![fid, sid.parse::<i64>().unwrap()])?;
        Ok(
            json!({"snapshot_id":sid,"function":function,"summaries":summaries,
            "page":page(10000,0,0),"analysis_available":!summaries.is_empty()}),
        )
    }

    /// Key lookup, independent of the list page size (including keys after 10,000).
    pub fn get_rule_evidence(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        key: &str,
    ) -> Result<Value> {
        let sid = self
            .closed_snapshot(snapshot, repository)?
            .parse::<i64>()
            .unwrap();
        let offset = one(self.db(),
            "SELECT (SELECT COUNT(*) FROM functional_rules WHERE snapshot_id=?1 AND rule_key<?2) AS position FROM functional_rules WHERE snapshot_id=?1 AND rule_key=?2",
            params![sid,key])?["position"].as_u64().unwrap() as usize;
        let mut listed = behavior::list_rules(self.db(), sid, true, 1, offset)?;
        let mut item = listed["rules"].as_array_mut().unwrap().remove(0);
        item["page"] = page(1, 0, 1);
        Ok(item)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn list_function_rules(
        &self,
        snapshot: &str,
        repository: Option<&str>,
        function: &str,
        with_evidence: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let sid = self.closed_snapshot(snapshot, repository)?;
        let (fid, _) = self.function(&sid, repository, function)?;
        let sid = sid.parse::<i64>().unwrap();
        let keys = rows(self.db(), "SELECT fr.rule_key FROM functional_rules fr JOIN behavior_cases bc ON bc.id=fr.behavior_case_id JOIN behavior_scenarios bs ON bs.id=bc.behavior_id WHERE fr.snapshot_id=?1 AND bs.entry_function_id=?2 ORDER BY fr.rule_key",params![sid,fid])?;
        let total = keys.len();
        let mut rules = Vec::new();
        for key in keys.into_iter().skip(offset).take(limit.min(10000)) {
            let mut rule = self.get_rule_evidence(
                &sid.to_string(),
                repository,
                key["rule_key"].as_str().unwrap(),
            )?;
            rule.as_object_mut().unwrap().remove("page");
            if !with_evidence {
                rule.as_object_mut().unwrap().remove("evidence");
            }
            rules.push(rule);
        }
        Ok(json!({"snapshot_id":sid,"rules":rules,"page":page(limit.min(10000),offset,total)}))
    }

    /// Structural fingerprints, not inventory counts. Missing fingerprints are explicit gaps.
    pub fn compare_snapshots(&self, repository: &str, from: &str, to: &str) -> Result<Value> {
        let a = self.closed_snapshot(from, Some(repository))?;
        let b = self.closed_snapshot(to, Some(repository))?;
        if a == b {
            return Err(Error::new("E_USAGE", "snapshots must differ"));
        }
        let load = |sid: &str| {
            rows(self.db(), "SELECT u.unit_key,s.qualified_name,f.structural_fingerprint FROM functions f JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id WHERE s.snapshot_id=?1 ORDER BY u.unit_key,s.qualified_name", [sid])
        };
        let left = load(&a)?;
        let right = load(&b)?;
        let key = |v: &Value| {
            format!(
                "{}::{}",
                v["unit_key"].as_str().unwrap(),
                v["qualified_name"].as_str().unwrap()
            )
        };
        let l: std::collections::BTreeMap<_, _> = left.iter().map(|v| (key(v), v)).collect();
        let r: std::collections::BTreeMap<_, _> = right.iter().map(|v| (key(v), v)).collect();
        let mut changes = Vec::new();
        for (k, v) in &l {
            match r.get(k) {
                None => changes.push(json!({"function":k,"change":"removed"})),
                Some(w) if v["structural_fingerprint"] != w["structural_fingerprint"] => {
                    changes.push(json!({"function":k,"change":"changed"}))
                }
                _ => {}
            }
        }
        for k in r.keys().filter(|k| !l.contains_key(*k)) {
            changes.push(json!({"function":k,"change":"added"}));
        }
        let incomplete = left
            .iter()
            .chain(&right)
            .any(|v| v["structural_fingerprint"].as_str().is_none());
        let total = changes.len();
        Ok(
            json!({"repository":repository,"from":a,"to":b,"changes":changes,"incomplete":incomplete,"page":page(10000,0,total)}),
        )
    }
}
