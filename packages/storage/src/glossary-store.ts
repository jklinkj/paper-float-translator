import { createGlossaryVersion, type Glossary } from "@paper-float-translator/core";
import { readJsonFile, writeJsonFile } from "./file-utils";

export class JsonGlossaryStore {
  constructor(private readonly filePath: string) {}

  async load(): Promise<Glossary> {
    const raw = await readJsonFile<Glossary>(this.filePath, {});
    return normalizeGlossary(raw);
  }

  async save(glossary: Glossary): Promise<Glossary> {
    const normalized = normalizeGlossary(glossary);
    await writeJsonFile(this.filePath, normalized);
    return normalized;
  }

  async version(): Promise<string> {
    return createGlossaryVersion(await this.load());
  }
}

function normalizeGlossary(raw: Glossary): Glossary {
  return Object.fromEntries(
    Object.entries(raw)
      .map(([source, target]) => [source.trim(), target.trim()])
      .filter(([source, target]) => source && target)
      .sort(([left], [right]) => left.localeCompare(right))
  );
}
