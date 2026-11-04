"""Check the library boundary's validation and annotation identities."""

import unittest

from pydantic import ValidationError

from annotations import Request, analyze


class BoundaryTests(unittest.TestCase):
    def test_invalid_requests_fail(self) -> None:
        for text in [
            '{"schema_version":2,"language":"en","segments":[]}',
            '{"schema_version":1,"language":"xx","segments":[]}',
            '{"schema_version":1,"language":"en","segments":[]}',
            '{"schema_version":1,"language":"en","segments":[],"extra":true}',
        ]:
            with self.assertRaises(ValidationError):
                Request.model_validate_json(text)

    def test_three_native_pipelines_produce_source_bound_annotations(self) -> None:
        for language, sentence in [
            ("en", "Tokyo has a railway museum."),
            ("ja", "東京には鉄道博物館があります。"),
            ("zh", "东京有一座铁路博物馆。"),
        ]:
            request = Request.model_validate(
                {
                    "schema_version": 1,
                    "language": language,
                    "segments": [{"id": "paragraph", "text": sentence}],
                }
            )
            result = analyze(request)
            self.assertEqual(len(result.annotations), 1)
            record = result.annotations[0]
            self.assertEqual(record.id, "paragraph")
            self.assertEqual(record.characters, len(sentence))
            self.assertTrue(record.tokens)
            self.assertTrue(record.sentences)
            self.assertTrue(any(token.dependency == "ROOT" for token in record.tokens))
            for token in record.tokens:
                self.assertEqual(sentence[token.start : token.end], token.text)
                self.assertLess(token.head, len(record.tokens))


if __name__ == "__main__":
    unittest.main()
