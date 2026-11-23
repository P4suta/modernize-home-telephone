use super::{Store, save_transaction};
use crate::model::Call;
use anyhow::{Context, ensure};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

pub enum ArchiveWork {
    Publish { bytes: u64 },
    Delete,
}

pub struct ArchiveTask {
    pub id: Uuid,
    pub work: ArchiveWork,
    pub attempts: u32,
}

pub struct ArchivedRecording {
    pub bytes: u64,
    pub digest: String,
}

impl Store {
    pub async fn pending_recordings(&self) -> anyhow::Result<u64> {
        self.with(|database| {
            let count:i64=database.query_row("SELECT COUNT(*) FROM calls WHERE ended_at IS NOT NULL AND json_extract(payload,'$.recording.status')='pending'",[],|row|row.get(0))?;
            Ok(u64::try_from(count)?)
        }).await
    }

    pub async fn archive_tasks(&self, now: u64) -> anyhow::Result<Vec<ArchiveTask>> {
        self.with(move |database| {
            database.execute("INSERT OR IGNORE INTO recording_archives(call_id,bytes,state,next_at) SELECT id,json_extract(payload,'$.recording.bytes'),'pending',0 FROM calls WHERE ended_at IS NOT NULL AND json_extract(payload,'$.recording.status')='available' AND json_extract(payload,'$.processing') IN ('complete','failed')",[])?;
            let mut query=database.prepare("SELECT call_id,state,bytes,attempts FROM recording_archives WHERE state!='available' AND next_at<=?1 ORDER BY next_at,call_id LIMIT 16")?;
            let rows=query.query_map([i64::try_from(now)?],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,u32>(3)?)))?;
            rows.map(|row| {
                let (id,state,bytes,attempts)=row?;
                let bytes=u64::try_from(bytes)?;
                let work=match state.as_str(){"pending"=>ArchiveWork::Publish{bytes},"deleting"=>ArchiveWork::Delete,_=>anyhow::bail!("invalid archive state")};
                Ok(ArchiveTask{id:Uuid::parse_str(&id)?,work,attempts})
            }).collect()
        }).await
    }

    pub async fn archived_recording(&self, id: Uuid) -> anyhow::Result<Option<ArchivedRecording>> {
        self.with(move |database| {
            let row:Option<(i64,String)>=database.query_row("SELECT bytes,digest FROM recording_archives WHERE call_id=?1 AND state='available'",[id.to_string()],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            row.map(|(bytes,digest)|Ok(ArchivedRecording{bytes:u64::try_from(bytes)?,digest})).transpose()
        }).await
    }

    pub async fn commit_archive(
        &self,
        id: Uuid,
        identity: ArchivedRecording,
        now: u64,
    ) -> anyhow::Result<bool> {
        ensure!(
            identity.bytes > 0
                && identity.digest.len() == 64
                && identity.digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid archive identity"
        );
        let events = self.events.clone();
        self.with(move |database| {
            let transaction=database.transaction()?;
            let payload:Option<String>=transaction.query_row("SELECT calls.payload FROM calls JOIN recording_archives ON recording_archives.call_id=calls.id WHERE calls.id=?1 AND recording_archives.state='pending' AND recording_archives.bytes=?2 AND calls.ended_at IS NOT NULL AND json_extract(calls.payload,'$.recording.status')='available'",params![id.to_string(),i64::try_from(identity.bytes)?],|row|row.get(0)).optional()?;
            let Some(payload)=payload else{return Ok(false)};
            let mut call:Call=serde_json::from_str(&payload)?;
            if call.last_error.as_deref()==Some("nas_archive_unavailable") {call.last_error=None;}
            let changed=transaction.execute("UPDATE recording_archives SET state='available',digest=?2 WHERE call_id=?1 AND state='pending'",params![id.to_string(),identity.digest])?;
            ensure!(changed==1,"archive publication lost its pending intent");
            let event=save_transaction(&transaction,&call,"recording_archived",now)?;
            transaction.commit()?;
            Self::committed(&events,event);
            Ok(true)
        }).await
    }

    pub async fn retry_archive(&self, id: Uuid, attempts: u32, now: u64) -> anyhow::Result<()> {
        let next = now
            .checked_add(5_u64 << attempts.min(6))
            .context("archive deadline overflow")?;
        let events = self.events.clone();
        self.with(move |database| {
            let transaction=database.transaction()?;
            transaction.execute("UPDATE recording_archives SET attempts=min(attempts+1,6),next_at=?2 WHERE call_id=?1 AND state!='available'",params![id.to_string(),i64::try_from(next)?])?;
            let payload:Option<String>=transaction.query_row("SELECT payload FROM calls WHERE id=?1 AND ended_at IS NOT NULL",[id.to_string()],|row|row.get(0)).optional()?;
            let event=if let Some(payload)=payload {
                let mut call:Call=serde_json::from_str(&payload)?;
                if call.last_error.is_none(){call.last_error=Some("nas_archive_unavailable".into());Some(save_transaction(&transaction,&call,"recording_archive_failed",now)?)}else{None}
            }else{None};
            transaction.commit()?;
            if let Some(event)=event{Self::committed(&events,event);}
            Ok(())
        }).await
    }

    pub async fn archive_deleted(&self, id: Uuid) -> anyhow::Result<()> {
        self.with(move |database| {
            database.execute(
                "DELETE FROM recording_archives WHERE call_id=?1 AND state='deleting'",
                [id.to_string()],
            )?;
            Ok(())
        })
        .await
    }
}
