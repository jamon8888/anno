#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "python-crfsuite>=0.9.10",
#     "datasets>=2.14.0",
# ]
# ///
"""
Train CRF NER weights on CoNLL-2003.

This script trains a CRF model using python-crfsuite on the CoNLL-2003 dataset
and exports the feature weights for use in the Rust CRF backend.

Usage:
    uv run scripts/train_crf_weights.py

Output:
    - crf_weights.json: Feature weights for Rust CRF backend
    - crf_model.crfsuite: Native CRFsuite model file
"""

import json
import pycrfsuite
from datasets import load_dataset
from collections import defaultdict


def word_shape(word: str) -> str:
    """Compute word shape (e.g., 'John' -> 'Xx', 'USA' -> 'X')."""
    shape = []
    prev = None
    for c in word:
        if c.isupper():
            ch = 'X'
        elif c.islower():
            ch = 'x'
        elif c.isdigit():
            ch = '0'
        else:
            ch = c
        if ch != prev:
            shape.append(ch)
            prev = ch
    return ''.join(shape)


def word2features(sent, i):
    """Extract features for a word at position i."""
    word = sent[i][0]
    
    features = [
        'bias',
        f'word.lower={word.lower()}',
        f'word.shape={word_shape(word)}',
        f'word.isdigit={word.isdigit()}',
        f'word.istitle={word.istitle()}',
        f'word.isupper={word.isupper()}',
    ]
    
    # Prefixes and suffixes
    if len(word) >= 2:
        features.append(f'prefix2={word[:2].lower()}')
        features.append(f'suffix2={word[-2:].lower()}')
    if len(word) >= 3:
        features.append(f'prefix3={word[:3].lower()}')
        features.append(f'suffix3={word[-3:].lower()}')
    
    # Context features
    if i > 0:
        word1 = sent[i-1][0]
        features.extend([
            f'-1:word.lower={word1.lower()}',
            f'-1:word.istitle={word1.istitle()}',
            f'-1:word.isupper={word1.isupper()}',
            f'-1:word.shape={word_shape(word1)}',
        ])
    else:
        features.append('BOS')
        
    if i < len(sent)-1:
        word1 = sent[i+1][0]
        features.extend([
            f'+1:word.lower={word1.lower()}',
            f'+1:word.istitle={word1.istitle()}',
            f'+1:word.isupper={word1.isupper()}',
            f'+1:word.shape={word_shape(word1)}',
        ])
    else:
        features.append('EOS')
    
    return features


def sent2features(sent):
    return [word2features(sent, i) for i in range(len(sent))]


def sent2labels(sent):
    return [label for token, _, label in sent]


def conll_to_sentences(dataset):
    """Convert HuggingFace CoNLL-2003 to list of sentences."""
    # NER tag mapping
    tag_map = {0: 'O', 1: 'B-PER', 2: 'I-PER', 3: 'B-ORG', 4: 'I-ORG', 
               5: 'B-LOC', 6: 'I-LOC', 7: 'B-MISC', 8: 'I-MISC'}
    
    sentences = []
    for example in dataset:
        tokens = example['tokens']
        ner_tags = example['ner_tags']
        pos_tags = example.get('pos_tags', [0] * len(tokens))
        
        sent = []
        for tok, pos, ner in zip(tokens, pos_tags, ner_tags):
            label = tag_map.get(ner, 'O')
            sent.append((tok, pos, label))
        sentences.append(sent)
    
    return sentences


def train_crf():
    """Train CRF model on CoNLL-2003."""
    print("Loading CoNLL-2003 dataset...")
    dataset = load_dataset("conll2003", trust_remote_code=True)
    
    print("Converting to sentences...")
    train_sents = conll_to_sentences(dataset['train'])
    test_sents = conll_to_sentences(dataset['test'])
    
    print(f"Train sentences: {len(train_sents)}")
    print(f"Test sentences: {len(test_sents)}")
    
    # Extract features
    print("Extracting features...")
    X_train = [sent2features(s) for s in train_sents]
    y_train = [sent2labels(s) for s in train_sents]
    X_test = [sent2features(s) for s in test_sents]
    y_test = [sent2labels(s) for s in test_sents]
    
    # Train CRF
    print("Training CRF model...")
    trainer = pycrfsuite.Trainer(verbose=True)
    
    for xseq, yseq in zip(X_train, y_train):
        trainer.append(xseq, yseq)
    
    trainer.set_params({
        'c1': 0.1,  # L1 regularization
        'c2': 0.1,  # L2 regularization
        'max_iterations': 100,
        'feature.possible_transitions': True,
    })
    
    trainer.train('crf_model.crfsuite')
    print("Model saved to crf_model.crfsuite")
    
    # Extract weights
    print("Extracting feature weights...")
    tagger = pycrfsuite.Tagger()
    tagger.open('crf_model.crfsuite')
    
    # Get state features (emission features)
    weights = {}
    info = tagger.info()
    
    for (attr, label), weight in info.state_features.items():
        key = f"{attr}:{label}"
        weights[key] = weight
    
    # Get transition features
    for (label_from, label_to), weight in info.transitions.items():
        key = f"trans:{label_from}->{label_to}"
        weights[key] = weight
    
    # Save weights as JSON
    with open('crf_weights.json', 'w') as f:
        json.dump(weights, f, indent=2)
    print(f"Saved {len(weights)} feature weights to crf_weights.json")
    
    # Evaluate
    print("\nEvaluating on test set...")
    y_pred = [tagger.tag(xseq) for xseq in X_test]
    
    # Calculate F1
    correct = 0
    predicted = 0
    actual = 0
    
    for y_true_seq, y_pred_seq in zip(y_test, y_pred):
        for y_true, y_pred_tag in zip(y_true_seq, y_pred_seq):
            if y_true != 'O':
                actual += 1
            if y_pred_tag != 'O':
                predicted += 1
            if y_true != 'O' and y_true == y_pred_tag:
                correct += 1
    
    precision = correct / predicted if predicted > 0 else 0
    recall = correct / actual if actual > 0 else 0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0
    
    print(f"Precision: {precision:.4f}")
    print(f"Recall: {recall:.4f}")
    print(f"F1: {f1:.4f}")
    
    return weights


if __name__ == '__main__':
    train_crf()

