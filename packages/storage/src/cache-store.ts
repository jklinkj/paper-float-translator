import type { CacheEntry } from "@paper-float-translator/core";
import { readJsonFile, writeJsonFile } from "./file-utils";

interface CacheFile {
  entries: Record<string, CacheEntry>;
}

export class JsonCacheStore {
  constructor(private readonly filePath: string) {}

  async get(key: string): Promise<CacheEntry | undefined> {
    const file = await this.read();
    return file.entries[key];
  }

  async set(entry: CacheEntry): Promise<void> {
    const file = await this.read();
    file.entries[entry.key] = entry;
    await writeJsonFile(this.filePath, file);
  }

  async clear(): Promise<void> {
    await writeJsonFile(this.filePath, { entries: {} satisfies Record<string, CacheEntry> });
  }

  private async read(): Promise<CacheFile> {
    const file = await readJsonFile<CacheFile>(this.filePath, { entries: {} });
    return file.entries && typeof file.entries === "object" ? file : { entries: {} };
  }
}
