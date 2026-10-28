from pathlib import Path
import subprocess, os, json, hashlib, time, sys
r=Path(__file__).resolve().parents[3]
e=Path(__file__).resolve().parent
env=os.environ.copy()
env["CARGO_TARGET_DIR"]=str(r/"target")
count=int(env.get("GIT_CONFIG_COUNT","0"))
env["GIT_CONFIG_KEY_"+str(count)]="core.longpaths"
env["GIT_CONFIG_VALUE_"+str(count)]="true"
env["GIT_CONFIG_COUNT"]=str(count+1)
files=["scripts/test-plan0014-wave5b.ps1","scripts/test-plan0014-wave5b-repairs.ps1"]
helper=r/"scripts/wave5b-safe-cleanup.cs"
if helper.exists(): files.append(str(helper.relative_to(r)))
digest=lambda:{f:hashlib.sha256((r/f).read_bytes()).hexdigest() for f in files}
initial=digest()
head=subprocess.check_output(["git","rev-parse","HEAD"],cwd=r,text=True).strip()
gates=[
 ("fmt",["cargo","fmt","--all","--","--check"]),
 ("check",["cargo","check","--workspace","--all-targets","--all-features"]),
 ("clippy",["cargo","clippy","--workspace","--all-targets","--all-features","--","-D","warnings"]),
 ("workspace",["cargo","test","--workspace","--all-features"]),
 ("architecture",["pwsh","-NoProfile","-File","scripts/check-architecture.ps1"]),
 ("repair",["pwsh","-NoProfile","-File","scripts/test-plan0014-wave5b-repairs.ps1","-Repo",str(r),"-OutDir",str(e/"final-repair")]),
 ("driver113",["pwsh","-NoProfile","-File","scripts/test-plan0014-wave5b.ps1","-Repo",str(r),"-EvidenceDir",str(e/"final-driver113"),"-Scratch",str(r/"target/q45-final-scratch")])
]
results=[]
for name,cmd in gates:
 if digest()!=initial: raise SystemExit("source changed before "+name)
 t=time.time()
 print(json.dumps({"gate":name,"status":"START","head":head,"command":cmd}),flush=True)
 with (e/("final-"+name+".log")).open("wb") as out:
  p=subprocess.Popen(cmd,cwd=r,env=env,stdout=out,stderr=subprocess.STDOUT)
  code=p.wait()
 result={"gate":name,"head":head,"command":cmd,"exit":code,"seconds":round(time.time()-t,1),"source_unchanged":digest()==initial}
 results.append(result)
 (e/"final-gate-results.json").write_text(json.dumps({"source_sha256":initial,"results":results},indent=2),encoding="utf-8")
 print(json.dumps(result),flush=True)
 if code or not result["source_unchanged"]: raise SystemExit(code or 1)
