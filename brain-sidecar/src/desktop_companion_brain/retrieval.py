from __future__ import annotations

import re


_STOP_WORDS = {'the', 'a', 'an', 'is', 'are', 'my', 'i', 'what', 'do', 'does', 'of', 'to', 'and'}
_STOP_BIGRAMS = {'我的', '什么', '怎么', '是否', '的是', '我想', '一下', '请问', '告诉', '现在'}


def terms(text: str) -> set[str]:
    result: set[str] = set()
    for token in re.findall(r'[a-z0-9]+|[\u3400-\u9fff]+', text.casefold()):
        if re.fullmatch(r'[a-z0-9]+', token):
            if token not in _STOP_WORDS:
                result.add(token)
        elif len(token) == 1:
            result.add(token)
        else:
            result.update(token[i:i + 2] for i in range(len(token) - 1)
                          if token[i:i + 2] not in _STOP_BIGRAMS)
    return result


def lexical_score(query: str, content: str) -> float:
    query_terms, content_terms = terms(query), terms(content)
    common = query_terms & content_terms
    if not common:
        return 0.0
    return len(common) / max(1, min(len(query_terms), len(content_terms)))
