"""Project validated spaCy annotations without editorial decisions."""

import hashlib
import importlib.metadata
import sys
from pathlib import Path
from typing import Literal

import spacy
from opencc import OpenCC
from pydantic import BaseModel, ConfigDict, Field
from spacy.about import __version__
from spacy.tokens import Doc, Span, Token
from thinc.util import fix_random_seed, require_cpu

SCHEMA = 1
ENGINE = "3.8.16"
MODEL_VERSION = "3.8.0"
SEED = 0
MAX_CHARACTERS = 100_000
MODELS = {
    "en": "en_core_web_sm",
    "ja": "ja_core_news_sm",
    "zh": "zh_core_web_sm",
}


class Record(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, frozen=True)


class Segment(Record):
    id: str = Field(min_length=1)
    text: str = Field(min_length=1, max_length=MAX_CHARACTERS)


class Request(Record):
    schema_version: Literal[1]
    language: Literal["en", "ja", "zh"]
    segments: list[Segment] = Field(min_length=1)


class Range(Record):
    start: int = Field(ge=0)
    end: int = Field(ge=0)


class Lexeme(Range):
    text: str
    lemma: str
    pos: str
    tag: str
    dependency: str
    head: int = Field(ge=0)
    morphology: str
    entity: str


class Entity(Range):
    label: str
    text: str


class Annotation(Record):
    id: str
    text_sha256: str
    characters: int
    tokens: list[Lexeme]
    sentences: list[Range]
    entities: list[Entity]
    simplified: str | None


class Response(Record):
    schema_version: int
    engine: str
    model: str
    model_version: str
    python_version: str
    packages: dict[str, str]
    annotations: list[Annotation]


def lexeme(token: Token) -> Lexeme:
    return Lexeme(
        start=token.idx,
        end=token.idx + len(token.text),
        text=token.text,
        lemma=token.lemma_,
        pos=token.pos_,
        tag=token.tag_,
        dependency=token.dep_,
        head=token.head.i,
        morphology=str(token.morph),
        entity=token.ent_type_,
    )


def entity(span: Span) -> Entity:
    return Entity(
        start=span.start_char, end=span.end_char, label=span.label_, text=span.text
    )


def annotation(identity: str, document: Doc, converter: OpenCC | None) -> Annotation:
    return Annotation(
        id=identity,
        text_sha256=hashlib.sha256(document.text.encode("utf-8")).hexdigest(),
        characters=len(document.text),
        tokens=[lexeme(token) for token in document],
        sentences=[
            Range(start=sentence.start_char, end=sentence.end_char)
            for sentence in document.sents
        ],
        entities=[entity(span) for span in document.ents],
        simplified=converter.convert(document.text) if converter is not None else None,
    )


def analyze(request: Request) -> Response:
    if __version__ != ENGINE:
        raise ValueError("spaCy differs from the pinned engine")
    model = MODELS[request.language]
    if importlib.metadata.version(model) != MODEL_VERSION:
        raise ValueError("the model differs from its pinned version")
    require_cpu()
    fix_random_seed(SEED)
    pipeline = spacy.load(model)
    converter = OpenCC("t2s") if request.language == "zh" else None
    documents = pipeline.pipe(
        (segment.text for segment in request.segments), batch_size=1, n_process=1
    )
    return Response(
        schema_version=SCHEMA,
        engine=__version__,
        model=model,
        model_version=MODEL_VERSION,
        python_version=".".join(str(part) for part in sys.version_info[:3]),
        packages={
            distribution.metadata["Name"]: distribution.version
            for distribution in importlib.metadata.distributions()
        },
        annotations=[
            annotation(segment.id, document, converter)
            for segment, document in zip(request.segments, documents, strict=True)
        ],
    )


def main() -> None:
    request = Request.model_validate_json(Path(sys.argv[1]).read_bytes())
    result = analyze(request)
    Path(sys.argv[2]).write_text(result.model_dump_json(), encoding="utf-8")


if __name__ == "__main__":
    main()
