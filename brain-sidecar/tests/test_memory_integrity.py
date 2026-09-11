from __future__ import annotations

import json
import tempfile
import threading
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import Mock, patch

from desktop_companion_brain.memory import Mem0Memory
from desktop_companion_brain.memory_inference import LocalMemoryInferenceProvider
from desktop_companion_brain.memory_manager import MemoryManager
from desktop_companion_brain.memory_policy import MemoryCandidate
from desktop_companion_brain.memory_store import MemoryEventStore
from desktop_companion_brain.session_store import ConversationSessionStore
from test_background_memory import wait_until
from test_memory_phase2 import FakeInference, FakeProvider, make_config, make_event, make_scope


class MemoryIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.scope = make_scope()
        self.provider = FakeProvider('replacement-framework')
        self.manager = MemoryManager(make_config(self.root), providers=[self.provider], inference=FakeInference([]))

    def tearDown(self):
        self.manager.close()
        self.temp.cleanup()

    def add(self, content, **kwargs):
        event = make_event(content, scope=self.scope, **kwargs)
        self.manager.store.add(event, ())
        return event

    def hit(self, event, **kwargs):
        return {'id': 'index-' + event.event_id, 'content': event.content, 'score': 0.9,
                'metadata': {'canonicalEventId': event.event_id, 'revision': event.revision}, **kwargs}

    def test_chinese_relevance_and_empty_result(self):
        self.add('喜欢喝咖啡')
        self.assertEqual(self.manager.search('我的生日是哪一天', self.scope, 5), [])
        self.assertEqual(self.manager.search('我喜欢喝什么咖啡', self.scope, 5)[0]['content'], '喜欢喝咖啡')

    def test_delete_and_edit_hide_stale_index_results_immediately(self):
        event = self.add('Prefers coffee')
        self.provider.search_records = [self.hit(event)]
        self.manager.update(event.event_id, 'Prefers tea', self.scope)
        self.assertEqual(self.manager.search('coffee', self.scope, 5), [])
        self.assertEqual(self.manager.search('tea', self.scope, 5)[0]['content'], 'Prefers tea')
        self.manager.delete(event.event_id, self.scope)
        self.assertEqual(self.manager.search('coffee', self.scope, 5), [])
        self.assertEqual(self.manager.list(self.scope), [])

    def test_edit_during_provider_search_does_not_publish_an_old_ranking(self):
        event = self.add('Likes coffee')
        def search(*args):
            self.manager.update(event.event_id, 'Likes tea', self.scope)
            return [self.hit(event)]
        self.provider.search = search
        self.assertEqual(self.manager.search('coffee', self.scope, 5), [])

    def test_unapproved_unknown_and_foreign_provider_results_are_not_recalled(self):
        pending = self.add('Pending fact', status='pending')
        foreign = make_event('Foreign fact', scope=make_scope('another'))
        self.manager.store.add(foreign, ())
        self.provider.search_records = [self.hit(pending), self.hit(foreign), {'id': 'unknown', 'content': 'fact', 'score': 1}]
        self.assertEqual(self.manager.search('fact', self.scope, 5), [])

    def test_graph_hits_require_current_canonical_sources(self):
        event = self.add('Prefers tea')
        self.provider.search_records = [{'id': 'edge', 'content': 'derived relation', 'canonicalEventIds': [event.event_id]}]
        self.assertEqual(self.manager.search('beverage', self.scope, 5), [])
        self.manager.store.link_provider_record(event.event_id, self.provider.provider_id, event.event_id)
        self.assertEqual(self.manager.search('beverage', self.scope, 5)[0]['content'], event.content)
        self.manager.delete(event.event_id, self.scope)
        self.assertEqual(self.manager.search('beverage', self.scope, 5), [])

    def test_episodes_with_different_dates_remain_distinct_in_storage_and_retrieval(self):
        first = replace(make_event('Went to the park', self.scope, kind='episodic'), occurred_at_unix_ms=1700000000000)
        second = replace(first, event_id='second', occurred_at_unix_ms=1700086400000)
        self.manager.store.add(first, ())
        self.manager.store.add(second, ())
        self.assertEqual(len(self.manager.search('park', self.scope, 5)), 2)

    def test_automatic_correction_replaces_fact_but_pending_correction_waits(self):
        old = self.add('Likes coffee')
        correction = replace(make_event('No longer drinks coffee', self.scope, status='pending'),
                             metadata={'supersedes': [old.event_id]})
        self.manager.store.add(correction, ())
        self.assertEqual(self.manager.search('coffee', self.scope, 5)[0]['content'], old.content)
        self.manager.approve(correction.event_id, self.scope)
        self.assertEqual([item['content'] for item in self.manager.search('coffee', self.scope, 5)], [correction.content])
        automatic = replace(make_event('Drinks coffee again', self.scope), metadata={'supersedes': [correction.event_id]})
        self.manager.store.add(automatic, ())
        self.assertEqual([item['content'] for item in self.manager.search('coffee', self.scope, 5)], [automatic.content])

    def test_receipt_prevents_rephrased_replay_and_deleted_memory_resurrection(self):
        inference = Mock()
        inference.extract.return_value = [MemoryCandidate(content='Prefers tea', importance=0.9,
            metadata={'evidence': 'I prefer tea', 'sourceRole': 'user', 'confidence': 0.9})]
        self.manager.inference = inference
        messages = [{'role': 'user', 'content': 'I prefer tea'}]
        self.manager.remember_turn(messages, self.scope, 'turn')
        memory = self.manager.list(self.scope)[0]
        self.manager.delete(memory['id'], self.scope)
        inference.extract.return_value = [MemoryCandidate(content='Enjoys tea', importance=0.9)]
        self.manager.remember_turn(messages, self.scope, 'turn')
        inference.extract.assert_called_once()
        self.assertEqual(self.manager.list(self.scope), [])

    def test_facts_and_receipt_rollback_together(self):
        store = self.manager.store
        events = [make_event('first', self.scope), make_event('second', self.scope)]
        queue = store._queue
        def fail_second(identifier, *args):
            if identifier == events[1].event_id:
                raise RuntimeError('simulated crash')
            return queue(identifier, *args)
        with patch.object(store, '_queue', side_effect=fail_second):
            with self.assertRaises(RuntimeError):
                store.apply_turn(self.scope, 'atomic', events, ['index'])
        self.assertEqual(store.list(self.scope), [])
        self.assertFalse(store.turn_processed(self.scope, 'atomic'))
        store.apply_turn(self.scope, 'atomic', events, ['index'])
        self.assertEqual(len(store.list(self.scope)), 2)
        self.assertTrue(store.turn_processed(self.scope, 'atomic'))

    def test_edit_and_delete_rollback_if_index_job_cannot_be_saved(self):
        event = self.add('Original fact')
        with patch.object(self.manager.store, '_queue', side_effect=RuntimeError('disk error')):
            with self.assertRaises(RuntimeError):
                self.manager.update(event.event_id, 'Changed fact', self.scope)
            self.assertEqual(self.manager.store.get(event.event_id, self.scope).content, event.content)
            with self.assertRaises(RuntimeError):
                self.manager.delete(event.event_id, self.scope)
            self.assertEqual(self.manager.store.get(event.event_id, self.scope).status, 'approved')

    def test_generic_legacy_import_requires_approval_and_does_not_resurrect_after_delete(self):
        record = {'id': 'legacy-id', 'content': 'Imported fact', 'score': 1}
        self.provider.list = lambda scope: [record]
        self.provider.search_records = [record]
        imported = self.manager.list(self.scope)
        self.assertEqual(len(imported), 1)
        self.assertEqual(imported[0]['status'], 'pending')
        self.assertEqual(self.manager.search('fact', self.scope, 5), [])
        self.manager.delete(imported[0]['id'], self.scope)
        self.assertEqual(self.manager.list(self.scope), [])
        self.assertEqual(self.manager.search('fact', self.scope, 5), [])

    def test_updated_timestamp_tracks_edit(self):
        event = self.add('Original fact')
        with patch('desktop_companion_brain.memory_store._now', return_value=event.created_at_unix_ms + 1000):
            self.manager.update(event.event_id, 'Edited fact', self.scope)
        memory = self.manager.list(self.scope)[0]
        self.assertNotEqual(memory['createdAt'], memory['updatedAt'])

    def test_changed_correction_target_rolls_back_approval(self):
        old = self.add('Original preference')
        correction = replace(make_event('Corrected preference', self.scope, status='pending'),
                             metadata={'supersedes': [old.event_id], 'supersedesVersions': {old.event_id: 1}})
        self.manager.store.add(correction, ())
        self.manager.update(old.event_id, 'Manually edited preference', self.scope)
        with self.assertRaises(ValueError):
            self.manager.approve(correction.event_id, self.scope)
        self.assertEqual(self.manager.store.get(old.event_id, self.scope).status, 'approved')
        self.assertEqual(self.manager.store.get(correction.event_id, self.scope).status, 'pending')

    def test_pending_corrections_cannot_both_replace_the_same_old_version(self):
        old = self.add('Old preference')
        corrections = [replace(make_event(text, self.scope, status='pending'),
                               metadata={'supersedes': [old.event_id], 'supersedesVersions': {old.event_id: 1}})
                       for text in ('First correction', 'Second correction')]
        for event in corrections:
            self.manager.store.add(event, ())
        self.manager.approve(corrections[0].event_id, self.scope)
        with self.assertRaises(ValueError):
            self.manager.approve(corrections[1].event_id, self.scope)
        self.assertEqual([event.event_id for event in self.manager.store.active(self.scope)], [corrections[0].event_id])

    def test_alternative_extractor_cannot_bypass_user_evidence_policy(self):
        inference = Mock()
        inference.extract.return_value = [MemoryCandidate(content='Unfounded fact', importance=0.9)]
        self.manager.inference = inference
        self.manager.remember_turn([{'role': 'user', 'content': 'hello'}], self.scope, 'unsupported')
        self.assertEqual(self.manager.list(self.scope), [])

    def test_rejected_import_schedules_index_deletion(self):
        record = {'id': 'unapproved-import', 'content': 'Legacy fact'}
        self.provider.list = lambda scope: [record]
        imported = self.manager.list(self.scope)[0]
        self.manager.reject(imported['id'], self.scope)
        wait_until(lambda: bool(self.provider.delete_calls))
        self.assertEqual(self.provider.delete_calls[0][0], record['id'])
        self.assertEqual(self.manager.list(self.scope), [])

    def test_worker_failure_is_visible_and_can_be_retried(self):
        with patch.object(self.manager, '_deliver_batch', side_effect=RuntimeError('disk unavailable')):
            self.manager._wake.set()
            wait_until(lambda: not self.manager._worker.is_alive())
            self.assertFalse(self.manager.status()['ready'])
            self.assertEqual(self.manager.status()['workerError'], 'RuntimeError')
        self.manager.retry(self.scope)
        self.assertTrue(self.manager.status()['workerAlive'])
        self.assertIsNone(self.manager.status()['workerError'])

    def test_pending_queue_status_and_manual_retry_are_scoped(self):
        sessions = ConversationSessionStore(self.root / 'queue.sqlite3')
        other = make_scope('other')
        try:
            for identifier, scope in [('mine', self.scope), ('other', other)]:
                sessions.commit(identifier, scope, 'hello', 'reply', {'text': 'reply'}, True)
                sessions.defer_memory_write(identifier, 'TimeoutError')
            status = sessions.memory_queue_status()
            self.assertEqual(status['pending'], 2)
            self.assertEqual(status['failedAttempts'], 2)
            sessions.retry_memory_writes(self.scope)
            self.assertEqual([item['turnId'] for item in sessions.pending_memory_writes()], ['mine'])
        finally:
            sessions.close()


class ExtractionEvidenceTests(unittest.TestCase):
    def test_only_user_evidence_and_known_replacement_ids_are_accepted(self):
        inference = object.__new__(LocalMemoryInferenceProvider)
        inference.model = 'local'
        inference.client = Mock()
        good = {'content': 'Does not drink coffee', 'importance': 0.9,
                'metadata': {'evidence': '我现在不喝咖啡', 'sourceRole': 'user', 'confidence': 0.9, 'supersedes': ['old']}}
        bad = [
            {**good, 'metadata': {**good['metadata'], 'sourceRole': 'assistant'}},
            {**good, 'metadata': {**good['metadata'], 'evidence': 'assistant guess'}},
            {**good, 'metadata': {**good['metadata'], 'confidence': 0.3}},
            {**good, 'metadata': {**good['metadata'], 'supersedes': ['foreign-id']}},
        ]
        inference.client.json.return_value = {'choices': [{'message': {'content': json.dumps({'memories': [good, *bad]})}}]}
        candidates = inference.extract(
            [{'role': 'user', 'content': '我现在不喝咖啡'}, {'role': 'assistant', 'content': 'assistant guess'}],
            make_scope(), 'turn', existing_memories=[{'id': 'old', 'content': 'Likes coffee'}],
        )
        self.assertEqual(len(candidates), 1)
        self.assertEqual(candidates[0].metadata['supersedes'], ['old'])


class Mem0AdapterContractTests(unittest.TestCase):
    def setUp(self):
        self.adapter = object.__new__(Mem0Memory)
        self.adapter._lock = threading.RLock()
        self.adapter._memory = Mock()
        self.scope = make_scope()
        self.adapter._memory.get.return_value = {'id': 'index-id', 'user_id': self.scope['userId'], 'agent_id': self.scope['characterId']}

    def test_update_refreshes_metadata_and_uses_direct_scoped_lookup(self):
        event = replace(make_event('Edited fact', self.scope), revision=3)
        self.adapter.upsert(event, 'index-id')
        self.adapter._memory.get_all.assert_not_called()
        self.assertEqual(self.adapter._memory.update.call_args.kwargs['metadata']['revision'], 3)
        self.assertEqual(self.adapter._memory.update.call_args.kwargs['metadata']['canonicalEventId'], event.event_id)
        self.adapter._memory.get.return_value['user_id'] = 'another-user'
        with self.assertRaises(ValueError):
            self.adapter.upsert(event, 'index-id')

    def test_search_uses_threshold_and_skips_a_busy_index(self):
        self.adapter._memory.search.return_value = {'results': []}
        self.adapter.search('tea', self.scope, 5)
        self.assertEqual(self.adapter._memory.search.call_args.kwargs['threshold'], 0.35)
        ready, release = threading.Event(), threading.Event()
        def hold_lock():
            with self.adapter._lock:
                ready.set()
                release.wait(timeout=2)
        thread = threading.Thread(target=hold_lock)
        thread.start()
        try:
            self.assertTrue(ready.wait(timeout=1))
            with self.assertRaises(RuntimeError):
                self.adapter.search('tea', self.scope, 5)
        finally:
            release.set()
            thread.join()


class InstalledMem0Tests(unittest.TestCase):
    def test_local_add_update_search_delete_and_recovery_mapping(self):
        with tempfile.TemporaryDirectory() as directory:
            config = make_config(Path(directory))
            adapter = Mem0Memory(config)
            try:
                self.assertTrue(adapter.ready()[0], adapter.ready()[1])
                memory = adapter._require()
                with (patch.object(memory.embedding_model, 'embed', return_value=[0.1] * config.embedding_dimensions),
                      patch.object(memory.llm, 'generate_response', side_effect=AssertionError('infer=False must not call LLM'))):
                    event = make_event('Original preference')
                    record_id = adapter.upsert(event, None)
                    edited = replace(event, content='Updated preference', revision=2)
                    self.assertEqual(adapter.upsert(edited, record_id), record_id)
                    self.assertEqual(adapter._record_for_event(edited), record_id)
                    records = adapter.search('preference', event.scope, 5)
                    self.assertEqual(records[0]['metadata']['revision'], 2)
                    self.assertEqual(records[0]['metadata']['canonicalEventId'], event.event_id)
                    adapter.delete_record(event.event_id, event.scope)
                    self.assertEqual(adapter.list(event.scope), [])
            finally:
                adapter.close()


if __name__ == '__main__':
    unittest.main()
