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
