#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "datasets>=2.14.0",
#     "numpy>=1.24.0",
# ]
# ///
"""
Train HMM NER weights on CoNLL-2003.

This script trains a Hidden Markov Model for NER using Maximum Likelihood Estimation
on the CoNLL-2003 dataset and exports the emission/transition probabilities for use 
in the Rust HMM backend.

Usage:
    uv run scripts/train_hmm_weights.py

Output:
    - hmm_weights.json: Emission and transition probabilities for Rust HMM backend
"""

import json
import numpy as np
from collections import defaultdict
from datasets import load_dataset


def word_features(word: str) -> dict:
    """Extract features from a word for emission smoothing."""
    return {
        'is_capitalized': word[0].isupper() if word else False,
        'is_all_caps': word.isupper() and len(word) > 1,
        'is_digit': word.isdigit(),
        'has_digit': any(c.isdigit() for c in word),
        'has_hyphen': '-' in word,
        'length': len(word),
    }


def conll_to_sentences(dataset):
    """Convert HuggingFace CoNLL-2003 to list of (token, label) sentences."""
    # NER tag mapping
    tag_map = {0: 'O', 1: 'B-PER', 2: 'I-PER', 3: 'B-ORG', 4: 'I-ORG', 
               5: 'B-LOC', 6: 'I-LOC', 7: 'B-MISC', 8: 'I-MISC'}
    
    sentences = []
    for example in dataset:
        tokens = example['tokens']
        ner_tags = example['ner_tags']
        
        sent = [(tok, tag_map.get(tag, 'O')) for tok, tag in zip(tokens, ner_tags)]
        sentences.append(sent)
    
    return sentences


def train_hmm():
    """Train HMM model on CoNLL-2003."""
    print("Loading CoNLL-2003 dataset...")
    dataset = load_dataset("conll2003", trust_remote_code=True)
    
    print("Converting to sentences...")
    train_sents = conll_to_sentences(dataset['train'])
    test_sents = conll_to_sentences(dataset['test'])
    
    print(f"Train sentences: {len(train_sents)}")
    print(f"Test sentences: {len(test_sents)}")
    
    # Count emission and transition frequencies
    print("Computing emission and transition counts...")
    
    # States (BIO tags)
    states = ['O', 'B-PER', 'I-PER', 'B-ORG', 'I-ORG', 'B-LOC', 'I-LOC', 'B-MISC', 'I-MISC']
    state_to_idx = {s: i for i, s in enumerate(states)}
    n_states = len(states)
    
    # Count matrices
    emission_counts = defaultdict(lambda: defaultdict(float))  # state -> word -> count
    transition_counts = np.zeros((n_states, n_states))
    initial_counts = np.zeros(n_states)
    state_counts = np.zeros(n_states)
    
    # Vocabulary
    vocab = set()
    
    for sent in train_sents:
        if not sent:
            continue
        
        # Initial state
        first_label = sent[0][1]
        initial_counts[state_to_idx[first_label]] += 1
        
        for i, (word, label) in enumerate(sent):
            word_lower = word.lower()
            vocab.add(word_lower)
            
            state_idx = state_to_idx[label]
            emission_counts[state_idx][word_lower] += 1
            state_counts[state_idx] += 1
            
            # Transitions
            if i > 0:
                prev_label = sent[i-1][1]
                prev_idx = state_to_idx[prev_label]
                transition_counts[prev_idx][state_idx] += 1
    
    print(f"Vocabulary size: {len(vocab)}")
    print(f"State counts: {dict(zip(states, state_counts))}")
    
    # Convert to probabilities with add-k smoothing
    print("Converting to probabilities...")
    
    k_emission = 0.001  # Smoothing for emissions
    k_transition = 0.1  # Smoothing for transitions
    
    # Emission probabilities
    emissions = {}
    for state_idx, state in enumerate(states):
        state_emissions = {}
        total = state_counts[state_idx] + k_emission * len(vocab)
        
        for word in vocab:
            count = emission_counts[state_idx].get(word, 0)
            prob = (count + k_emission) / total
            if prob > 0.001:  # Only store significant probabilities
                state_emissions[word] = prob
        
        emissions[state] = state_emissions
    
    # Transition probabilities
    transitions = {}
    for i, from_state in enumerate(states):
        row_total = transition_counts[i].sum() + k_transition * n_states
        for j, to_state in enumerate(states):
            count = transition_counts[i][j]
            prob = (count + k_transition) / row_total
            transitions[f"{from_state}->{to_state}"] = prob
    
    # Initial probabilities
    initial_total = initial_counts.sum() + k_transition * n_states
    initial = {state: (initial_counts[i] + k_transition) / initial_total 
               for i, state in enumerate(states)}
    
    # Feature-based backoff probabilities for unknown words
    print("Computing backoff probabilities...")
    feature_emissions = defaultdict(lambda: defaultdict(float))
    feature_counts = defaultdict(lambda: defaultdict(int))
    
    for sent in train_sents:
        for word, label in sent:
            feats = word_features(word)
            for feat_name, feat_val in feats.items():
                if isinstance(feat_val, bool) and feat_val:
                    feature_counts[feat_name][label] += 1
                    feature_emissions[feat_name]['_total'] += 1
    
    # Normalize feature probabilities
    backoff = {}
    for feat_name, label_counts in feature_counts.items():
        total = sum(label_counts.values())
        if total > 10:  # Only use features with enough data
            backoff[feat_name] = {label: count / total for label, count in label_counts.items()}
    
    # Build output
    model = {
        'states': states,
        'emissions': emissions,
        'transitions': transitions,
        'initial': initial,
        'backoff': backoff,
        'smoothing': k_emission,
    }
    
    # Save weights as JSON
    with open('hmm_weights.json', 'w') as f:
        json.dump(model, f, indent=2)
    
    emission_count = sum(len(e) for e in emissions.values())
    print(f"Saved HMM weights to hmm_weights.json")
    print(f"  Emissions: {emission_count}")
    print(f"  Transitions: {len(transitions)}")
    print(f"  Initial: {len(initial)}")
    print(f"  Backoff features: {len(backoff)}")
    
    # Evaluate
    print("\nEvaluating on test set...")
    correct = 0
    predicted = 0
    actual = 0
    
    for sent in test_sents:
        if not sent:
            continue
        
        words = [w for w, _ in sent]
        labels = [l for _, l in sent]
        
        # Viterbi decoding
        pred_labels = viterbi_decode(words, model)
        
        for y_true, y_pred in zip(labels, pred_labels):
            if y_true != 'O':
                actual += 1
            if y_pred != 'O':
                predicted += 1
            if y_true != 'O' and y_true == y_pred:
                correct += 1
    
    precision = correct / predicted if predicted > 0 else 0
    recall = correct / actual if actual > 0 else 0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) > 0 else 0
    
    print(f"Precision: {precision:.4f}")
    print(f"Recall: {recall:.4f}")
    print(f"F1: {f1:.4f}")
    
    return model


def viterbi_decode(words, model):
    """Simple Viterbi decoder for HMM."""
    states = model['states']
    emissions = model['emissions']
    transitions = model['transitions']
    initial = model['initial']
    backoff = model['backoff']
    smoothing = model['smoothing']
    
    n_states = len(states)
    n_words = len(words)
    
    if n_words == 0:
        return []
    
    # Viterbi matrices (log probabilities)
    viterbi = np.full((n_states, n_words), -np.inf)
    backpointer = np.zeros((n_states, n_words), dtype=int)
    
    # Initialize
    for i, state in enumerate(states):
        word_lower = words[0].lower()
        emit_prob = emissions.get(state, {}).get(word_lower, smoothing)
        
        # Backoff for unknown words
        if word_lower not in emissions.get(state, {}):
            feats = word_features(words[0])
            for feat_name, feat_val in feats.items():
                if isinstance(feat_val, bool) and feat_val and feat_name in backoff:
                    emit_prob = max(emit_prob, backoff[feat_name].get(state, smoothing))
        
        viterbi[i, 0] = np.log(initial[state]) + np.log(max(emit_prob, 1e-10))
    
    # Forward pass
    for t in range(1, n_words):
        word_lower = words[t].lower()
        for j, to_state in enumerate(states):
            emit_prob = emissions.get(to_state, {}).get(word_lower, smoothing)
            
            # Backoff for unknown words
            if word_lower not in emissions.get(to_state, {}):
                feats = word_features(words[t])
                for feat_name, feat_val in feats.items():
                    if isinstance(feat_val, bool) and feat_val and feat_name in backoff:
                        emit_prob = max(emit_prob, backoff[feat_name].get(to_state, smoothing))
            
            log_emit = np.log(max(emit_prob, 1e-10))
            
            best_score = -np.inf
            best_prev = 0
            for i, from_state in enumerate(states):
                trans_key = f"{from_state}->{to_state}"
                trans_prob = transitions.get(trans_key, smoothing)
                score = viterbi[i, t-1] + np.log(max(trans_prob, 1e-10)) + log_emit
                if score > best_score:
                    best_score = score
                    best_prev = i
            
            viterbi[j, t] = best_score
            backpointer[j, t] = best_prev
    
    # Backtrack
    path = []
    best_last = int(np.argmax(viterbi[:, -1]))
    path.append(states[best_last])
    
    current = best_last
    for t in range(n_words - 1, 0, -1):
        current = backpointer[current, t]
        path.append(states[current])
    
    path.reverse()
    return path


if __name__ == '__main__':
    train_hmm()

