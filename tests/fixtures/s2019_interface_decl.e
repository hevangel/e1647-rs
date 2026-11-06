<'
interface printable { to_text(): string; };
interface sized { size_of(): uint; };
interface both like printable, sized { describe(prefix: string): string; };
'>
