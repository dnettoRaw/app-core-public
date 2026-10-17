import test from 'node:test';import assert from 'node:assert/strict';
import {initial,command,step} from '../runner/model.mjs';
import * as scene from '../scene3d/model.mjs';
test('runner rejects stale commands, pauses physics and restarts after collision',()=>{
 const state=initial();assert.equal(command(state,'jump',99),false);assert.equal(state.phase,'ready');command(state,'jump');step(state,1/60);assert.ok(state.y>0);
 command(state,'pause');const ticks=state.ticks;step(state,1/60);assert.equal(state.ticks,ticks);command(state,'pause');
 for(let i=0;i<1000&&state.phase==='playing';i++)step(state,1/60);assert.equal(state.phase,'crashed');const best=state.best;command(state,'restart');assert.equal(state.phase,'ready');assert.equal(state.best,best);
 assert.throws(()=>{command(state,'jump');step(state,1);},RangeError);
});
test('runner simulation is deterministic and obstacle retention stays bounded',()=>{const a=initial(),b=initial();command(a,'jump');command(b,'jump');for(let i=0;i<700;i++){step(a,1/60);step(b,1/60);assert.ok(a.obstacles.length<=8);}assert.deepEqual(a,b);});
test('3D matrices preserve identity and perspective bounds; state commands are revision checked',()=>{
 const translated=scene.translate(1,2,3);assert.deepEqual(scene.multiply(scene.identity(),translated),translated);assert.equal(translated[12],1);assert.equal(translated[14],3);assert.throws(()=>scene.perspective(0),RangeError);assert.ok([...scene.perspective(2)].every(Number.isFinite));
 const state=scene.initial();assert.equal(scene.command(state,'explode',9),false);assert.equal(state.explode,false);scene.command(state,'explode');assert.equal(state.explode,true);for(let i=0;i<100;i++)scene.command(state,'near');assert.equal(state.distance,7);for(let i=0;i<100;i++)scene.command(state,'far');assert.equal(state.distance,20);
});
test('Lorenz RK4 preserves equilibria and bounded deterministic ring buffers',()=>{
 assert.deepEqual(scene.rk4([0,0,0]),[0,0,0]);
 const q=Math.sqrt((8/3)*27),equilibrium=scene.rk4([q,q,27]);for(let i=0;i<3;i++)assert.ok(Math.abs(equilibrium[i]-[q,q,27][i])<1e-12);
 assert.throws(()=>scene.rk4([1,1,1],1),RangeError);
 const a=scene.createLorenz(),b=scene.createLorenz();assert.deepEqual(a.points,b.points);assert.equal(a.points.length,scene.TRAILS*scene.TRAIL_LENGTH*3);
 for(let i=0;i<1000;i++)scene.advanceLorenz(a,8);assert.equal(a.count,scene.TRAIL_LENGTH);assert.ok(a.cursor>=0&&a.cursor<scene.TRAIL_LENGTH);assert.ok([...a.points].every(Number.isFinite));assert.throws(()=>scene.advanceLorenz(a,9),RangeError);
});


test('showcase geometry is deterministic, finite and bounded with valid triangles', async()=>{
 const {createShowcase}=await import('../scene3d/showcase.mjs');
 for(const kind of ['galaxy','terrain','waves']){
  const a=createShowcase(kind),b=createShowcase(kind);
  assert.deepEqual(a.points,b.points);
  assert.ok(a.points.every(Number.isFinite));
  assert.ok(a.points.every(v=>Math.abs(v)<=5));
  assert.equal(a.points.length,kind==='galaxy'?36000:73728);
  if(a.triangles){for(let i=0;i<a.points.length;i+=9){
   const x1=a.points[i+3]-a.points[i],z1=a.points[i+5]-a.points[i+2];
   const x2=a.points[i+6]-a.points[i],z2=a.points[i+8]-a.points[i+2];
   assert.ok(Math.abs(x1*z2-z1*x2)>0);
  }}
 }
 assert.throws(()=>createShowcase('unknown'),RangeError);
});


test('recursive city respects budget and has unit outward face normals',async()=>{
 const {createCity}=await import('../scene3d/city.mjs');const city=createCity();
 assert.deepEqual(city,createCity());assert.equal(city.boxes,391);
 assert.equal(city.points.length,city.boxes*108);assert.equal(city.normals.length,city.points.length);
 assert.ok(city.points.every(Number.isFinite));assert.ok(city.points.every(v=>Math.abs(v)<=26));
 for(let i=0;i<city.points.length;i+=9){
  const a=Array.from(city.points.slice(i,i+3)),b=Array.from(city.points.slice(i+3,i+6)),c=Array.from(city.points.slice(i+6,i+9));
  const u=b.map((v,j)=>v-a[j]),v=c.map((v,j)=>v-a[j]);
  const cross=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
  const n=Array.from(city.normals.slice(i,i+3));assert.equal(Math.hypot(...n),1);
  assert.ok(cross.reduce((sum,x,j)=>sum+x*n[j],0)>0);
 }
});
test('flight moves relative to camera with bounded speed, boost and coordinates',async()=>{
 const {flightInitial,moveFlight}=await import('../scene3d/city.mjs');
 const a=flightInitial();a.pitch=0;moveFlight(a,new Set(['KeyW']),.04);assert.ok(Math.abs(a.position[2]-17.88)<1e-8);
 const b=flightInitial();b.pitch=0;moveFlight(b,new Set(['KeyW','KeyD']),.04);assert.ok(Math.abs(Math.hypot(b.position[0],b.position[2]-18)-.12)<1e-8);
 const c=flightInitial();c.pitch=0;c.yaw=Math.PI/2;moveFlight(c,new Set(['KeyW','ShiftLeft']),.04);assert.ok(Math.abs(c.position[0]+.36)<1e-8);
 moveFlight(c,new Set(),.04);assert.ok(Math.abs(c.position[0]+.36)<1e-8);
 for(let i=0;i<1000;i++)moveFlight(c,new Set(['KeyW','KeyQ']),.04);assert.ok(c.position.every(v=>Math.abs(v)<=40));assert.ok(c.position[1]>=.3);
 assert.throws(()=>moveFlight(c,new Set(),1),RangeError);
});


test('WebGL shared uniforms and varyings have matching explicit precision',async()=>{
 const {readFile}=await import('node:fs/promises');
 const source=await readFile(new URL('../scene3d/page.js',import.meta.url),'utf8');
 const vertex=source.match(/shader\(gl.VERTEX_SHADER,'([^']+)'\)/)[1];
 const fragment=source.match(/shader\(gl.FRAGMENT_SHADER,'([^']+)'\)/)[1];
 const declarations=shader=>new Map([...shader.matchAll(/(uniform|varying)\s+(?:(lowp|mediump|highp)\s+)?(\w+)\s+(\w+)\s*;/g)].map(m=>[m[4],{storage:m[1],precision:m[2],type:m[3]}]));
 const vs=declarations(vertex),fs=declarations(fragment);
 for(const [name,decl] of fs){if(vs.has(name)){assert.ok(decl.precision,`explicit precision: ${name}`);assert.deepEqual(decl,vs.get(name),`stage interface: ${name}`);}}
 assert.equal(vs.get('cityMode').precision,'mediump');
});
