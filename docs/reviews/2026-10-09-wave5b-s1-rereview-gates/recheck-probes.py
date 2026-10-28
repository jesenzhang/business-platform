from pathlib import Path
import subprocess,json,hashlib,shutil
root=Path('F:/Workspace/business-platform')
c=root/'target/w5b-review-1009'
base=root/'target/w5b-review-1009-probes'
e=root/'docs/reviews/2026-10-09-wave5b-s1-rereview-gates'
binary=root/'target/w5b-review-1009-build/debug/module-release-tool.exe'
assert binary.exists(), binary
cases={
 'control_foreign_alias':'INSERT INTO roles SELECT r.* FROM roles r, platform_users p;',
 'foreign_after_alias_column_list':'INSERT INTO roles (role_id) SELECT r.id FROM roles AS r(id), platform_users p;',
 'foreign_after_join_on':'INSERT INTO roles (role_id) SELECT r.role_id FROM roles r JOIN role_permissions rp ON rp.role_id = r.role_id, platform_users p;',
 'delete_foreign_after_alias_column_list':'DELETE FROM roles USING roles AS r(id), platform_users p;',
 'foreign_after_inheritance_star':'INSERT INTO roles SELECT r.* FROM roles * r, platform_users p;',
 'control_same_owner':'INSERT INTO roles SELECT r.* FROM roles r, role_permissions rp;',
}
results=[]
for name,sql in cases.items():
 repo=base/name; dest=repo/'migrations/modules/policy'; dest.mkdir(parents=True,exist_ok=False)
 template=c/'tools/module-release-tool/tests/fixtures/wave5b/catalogs/r2-foreign-in-from-alias/policy/catalog.json'
 shutil.copyfile(template,dest/'catalog.json')
 data=(sql+'\n').encode('utf-8'); file='001_probe.sql'; (dest/file).write_bytes(data)
 (dest/'MANIFEST.sha256').write_bytes((hashlib.sha256(data).hexdigest()+'  '+file+'\n').encode())
 cmd=[str(binary),'catalog-check','--repo',str(repo),'--only','module-catalog']
 r=subprocess.run(cmd,cwd=c,capture_output=True,text=True,encoding='utf-8')
 results.append({'case':name,'kind':'sql','input':sql,'command':cmd,'exit':r.returncode,'output':r.stdout+r.stderr})
embed_cases={
 'control_direct':'static M: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");',
 'control_alias':'use sqlx::migrate as embed; static M: sqlx::migrate::Migrator = embed!("../../migrations");',
 'raw_alias':'use sqlx::migrate as r#embed; static M: sqlx::migrate::Migrator = r#embed!("../../migrations");',
 'raw_macro_import':'use sqlx::r#migrate as embed; static M: sqlx::migrate::Migrator = embed!("../../migrations");',
 'double_quote_char': """const QUOTE: char = '"'; use sqlx::migrate as embed; static M: sqlx::migrate::Migrator = embed!("../../migrations");""",
}
for name,text in embed_cases.items():
 repo=base/('embed-'+name)
 (repo/'migrations').mkdir(parents=True,exist_ok=False)
 for crate in ['runtime-migration','extra']:
  d=repo/'crates'/crate; (d/'src').mkdir(parents=True)
  (d/'Cargo.toml').write_bytes(('[package]\nname="'+crate+'"\nversion="0.1.0"\nedition="2021"\n').encode())
 (repo/'crates/runtime-migration/src/lib.rs').write_bytes(b'static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");\n')
 (repo/'crates/extra/src/lib.rs').write_bytes((text+'\n').encode())
 cmd=[str(binary),'catalog-check','--repo',str(repo),'--only','baseline-embed']
 r=subprocess.run(cmd,cwd=c,capture_output=True,text=True,encoding='utf-8')
 results.append({'case':name,'kind':'embed','input':text,'command':cmd,'exit':r.returncode,'output':r.stdout+r.stderr})
(e/'catalog-embed-probes.json').write_text(json.dumps(results,ensure_ascii=False,indent=2),encoding='utf-8')
for item in results: print(item['kind'],item['case'],'exit='+str(item['exit']),item['output'].strip().replace('\n',' ')[:500])
