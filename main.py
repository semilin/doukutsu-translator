from openai import OpenAI
from pydantic import BaseModel
from pathlib import Path
import argparse
import json
import os
import sys

key = os.getenv("OPENROUTER_API_KEY")
if not key:
    print("OPENROUTER_API_KEY environment variable must be set.")
    sys.exit(1)
client = OpenAI(
    base_url="https://openrouter.ai/api/v1",
    api_key=key,
)

MODEL = "google/gemini-3-flash-preview"
# MODEL = 'gemma3:4b'
CONTEXT = (
    f"### CONTEXT\nYou are a translator for 'Cave Story' into Classical Latin.\n\n"
)


def data_summary(data):
    dialogues = data["dialogues"]
    return [{s["character"]: t[0]} for d in dialogues for s in d for t in s["text"]]


def json_parse(s):
    if s.startswith("```json"):
        s = s[8:-3]
    parsed = json.loads(s)
    return parsed


def get_summary(ds):
    response = client.chat.completions.create(
        model=MODEL,
        messages=[
            {
                "role": "user",
                "content": (
                    CONTEXT + f"### DATA TO ANALYZE\n"
                    f"{ds}\n\n"
                    f"### INSTRUCTIONS\n"
                    f"1. Scan the DATA for all isolated names and terms that should be translated consistently: characters, places, recurring items, etc.. Write the original English names for now. Don't include any dialogue.\n"
                    f"2. For each named speaking character, write a short description of how you are going to translate the character's style into Latin.\n"
                    f"3. Write a short and strict translation style guide for yourself. Include rules for transliteration (e.g., how to handle foreign letters), formatting, spelling (e.g., when to use j/i, v/u?), etc. Mention to never use macrons, as this game only uses ASCII."
                    f"4. Ignore technical markers like 'NP'.\n\n"
                    f"### OUTPUT RULE\n"
                    f"Return a JSON dictionary like so: {{'terms': ['Polar Star', ...], 'character_styles': {{'Curly Brace': '...', ...}}, 'style_guide': {{...}}}}."
                ),
            }
        ],
        response_format={"type": "json_object"},
    )
    cont = response.choices[0].message.content
    return (json_parse(cont), response.usage.cost)


def get_term_translations(terms):
    prompt = (
        CONTEXT + f"### Terms to Translate\n"
        f"{json.dumps(terms)}\n\n"
        f"### Instructions\n"
        f"1. Translate these terms into Classical Latin.\n"
        f"2. CRITICAL GRAMMAR RULE: Provide the NOMINATIVE SINGULAR form for nouns. Do not inflect them yet. \n"
        f"3. Do not use crude neologisms (e.g., use 'Automaton' for Robot, not 'Robotus').\n"
        f"4. For weapons, use standard military terminology (e.g., 'Lamina' or 'Gladius' for Blade).\n\n"
        f"### Output Format\n"
        f'Return a JSON dictionary: {{"English Term": "Latin Nominative"}}'
    )
    response = client.chat.completions.create(
        model=MODEL,
        messages=[
            {
                "role": "user",
                "content": prompt,
            }
        ],
        response_format={"type": "json_object"},
    )
    return (json_parse(response.choices[0].message.content), response.usage.cost)

def translate_dialogue(summary, previous_dialogues, dialogue):
    pattern = [len(list(s.values())[0]) for s in dialogue]
    segment_instructions = (
        f"**Segment**: Split the Latin translation back into segments. \n"
        f"   - HARD CONSTRAINT: Each line must be under 34 characters if the portrait is 'NP', else under 27 characters.\n"
        f"   - If a sentence or clause could fit under the constraint, don't add unnecessary linebreaks.\n"
        f"   - You may use standard Latin abbreviations (e.g., 'n.' for 'non', 'est' omitted) to fit the limit.\n"
        f"   - Do not break words in half unless necessary (if necessary, use a hyphen).\n"
        f"   - There must be exactly the same number of speeches as in the English. ({len(dialogue)})\n"
        f"   - Each speech list must be broken up exactly like the English, and must contain exactly the same number of items as in the English. ({json.dumps(pattern)})\n"
        f"   - The line breaks within each item, however, may be adjusted (but must still use `\\r\\n` every time).\n"
        f"   - Count your segmented version before outputting to ensure that it fits the constraints.\n"
    )

    prompt = (
        CONTEXT + f"### Global Glossary (Must Use)\n"
        f"{json.dumps(summary['terms'])}\n\n"
        f"### Character Styles\n"
        f"{json.dumps(summary['character_styles'])}\n\n"
        f"### Style Guide\n"
        f"{json.dumps(summary['style_guide'])}\n\n"
        f"### Example Input\n"
        f'[{{"Char1": ["I see.\r\nI can\'t do this myself.", "Can you?"]}}, {{"Char2": ["Yes."]}}]\n\n'
        f"### Example Output\n"
        f'[["Video.\r\Hoc facere solus nequeo.", "Potesne?"], ["Possum."]]\n'
        f"### Instructions\n"
        f"1. **Check Context**: Look at the Preceding Dialogues and determine if they provide relevant context or are unrelated. Be wary that, due to the structure of the dialogue files, dialogues may be only coincidentally adjacent.\n"
        f"2. **Analyze**: Determine the grammatical structure (Subject, Object, Verb) of the Dialogue to Translate.\n"
        f"3. **Translate**: Translate the thought into idiomatic Classical Latin. Avoid literalism (e.g., 'Ede clavem' is wrong; use 'Dede' or 'Trade'). Fix morphology (e.g., 'Interfice' not 'Redinterface'). Ensure all words are real, properly inflected Latin words (avoid neologisms unless absolutely necessary for clarity). Overall, prioritize Latin accuracy and intelligibility over structural fidelity to the English. Adhere to the style guide.\n"
        f"4. {segment_instructions}\n"
        f"5. **Output**: Return JSON with the new dialogue. Write a nested list [[\"...\"]] with ONLY dialogue, no dictionary keys ([{{\"NP\": [\"...\"]}}] -> [[\"...\"]]). Follow the example given.\n\n"
        f"### Preceding Dialogues in File\n"
        f"{json.dumps(previous_dialogues)}\n\n"
        f"### Dialogue to Translate\n"
        f"{json.dumps(dialogue)}"
    )

    cost = 0
    attempts = 0
    while True:
        attempts += 1
        if attempts > 10:
            sys.exit(1)
        try:
            response = client.chat.completions.create(
                model=MODEL,
                messages=[
                    {
                        "role": "user",
                        "content": prompt,
                    }
                ],
                response_format={"type": "json_object"},
            )
            cost += response.usage.cost
            parsed = json_parse(response.choices[0].message.content)
            if (len(parsed) != len(dialogue)
                or any([len(s) != len(list(dialogue[i].values())[0]) for (i, s) in enumerate(parsed)])
                or any(type(s) != type([]) for s in parsed)):
                print(f"Valid JSON, but not the right format: {parsed}")
                translation = parsed
                prompt = (
                    f"### Segmenting Directions\n{segment_instructions}\n"
                    f"### Instructions\n"
                    f"This Latin translation for the game Cave Story does not properly meet the constraints of the original English dialogue. The English length schema is {pattern}, but the Latin is {[len(s) for s in translation]}. Think how to align the Latin to the English, and construct one that follows the length schema. Carefully count your result before returning it.\n"
                    f"**Output**: Return JSON with the new dialogue. Write a nested list [[\"...\"]] with ONLY dialogue, no dictionary keys.\n\n"
                    f"### ENGLISH\n{json.dumps(dialogue)}\n\n"
                    f"### LATIN\n{json.dumps(translation)}\n\n"
                )
                print(prompt)
                continue
            break
        except KeyboardInterrupt:
            sys.exit(0)
        except Exception as e:
            print(f"Error: {type(e).__name__} {e}\nTrying again.")
            pass
    return (parsed, cost)

def format_dialogue(dl):
    return [{s['character']: [t[0] for t in s['text']]} for s in dl]

def make_translation(input: Path, output: Path):
    cost = 0.0
    with open(input, "r") as f:
        j = json.load(f)
        files = j['files']
        ds = [data_summary(d) for d in files]
        summary, sumcost = get_summary(ds)
        cost += sumcost
        term_translations = get_term_translations(summary["terms"])
        summary["terms"], termcost = term_translations
        cost += termcost
        print(summary)
        dialogue_count = sum([len(data['dialogues']) for data in files])
        idx = 0
        print(f"Current cost: ${cost:.4f}")
        for data in files:
            ds = data_summary(data)
            for (i, dl) in enumerate(data["dialogues"]):
                idx += 1
                print(f"{idx}/{dialogue_count} ({float(idx)/float(dialogue_count)*100:.2f}%), ${cost:.4f}")
                formatted = format_dialogue(dl)
                context = [format_dialogue(d) for d in data["dialogues"][max(0, i-3): i]]
                print(data["dialogues"][i])
                trans, tcost = translate_dialogue(summary, context, formatted)
                cost += tcost
                for (x, s) in enumerate(trans):
                    for (y, t) in enumerate(s):
                        dl[x]["text"][y][0] = t
                print(data["dialogues"][i])
            print(data)
        with open(output, "w") as out:
            json.dump(j, out, indent=2)

def main():
    parser = argparse.ArgumentParser(
        prog='translate.py',
        description='Given a dialogue JSON file, uses an LLM to output a new dialogue file translated into the target language.'
    )
    subparsers = parser.add_subparsers()
    translate_parser = subparsers.add_parser('t', help='translate game dialogue file')
    translate_parser.add_argument('input', type=Path, help='the JSON file with the original game dialogue')
    translate_parser.add_argument('output', type=Path, help='the JSON file to be written to with the new translation')
    translate_parser.set_defaults(func=lambda ns: make_translation(ns.input, ns.output))
    args = parser.parse_args()
    args.func(args)

if __name__ == "__main__":
    main()
